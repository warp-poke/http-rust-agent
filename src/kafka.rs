use futures::Future;
use futures::Stream;
use futures::future::lazy;

use futures_cpupool::Builder;
use tokio_core::reactor::Core;
use warp10;

use rdkafka::Message;
use rdkafka::config::ClientConfig;
use rdkafka::consumer::Consumer;
use rdkafka::consumer::stream_consumer::StreamConsumer;
use rdkafka::producer::FutureProducer;

use serde_json;
use time;

use BufferedDomainTestResult;
use RequestBenchEvent;
use check::run_check_for_url;
use warp10_post;

//FIXME: send back an error
fn check_and_post(payload: &[u8], warp10_url: &str, warp10_token: &str) -> Result<(), String> {
    let r: serde_json::Result<RequestBenchEvent> = serde_json::from_slice(payload);
    match r {
        Ok(request) => {
            println!("got request: {:#?}", request);
            let check = run_check_for_url(&request.url, true);

            let result = BufferedDomainTestResult {
                domain_test_results: vec![check],
                timestamp: time::now_utc().to_timespec(),
                request_bench_event: request,
            };

            let data: Vec<warp10::Data> = result.into();
            println!("sending to warp10: {:?}", data);

            let res = warp10_post(data, warp10_url.to_string(), warp10_token.to_string());
            println!("{:#?}", res);
            match res {
                Ok(_) => Ok(()),
                Err(e) => Err(format!("{:?}", e)),
            }
        },
        Err(e) => Err(format!("{:?}", e)),

    }
}

// Creates all the resources and runs the event loop. The event loop will:
//   1) receive a stream of messages from the `StreamConsumer`.
//   2) filter out eventual Kafka errors.
//   3) send the message to a thread pool for processing.
//   4) produce the result to the output topic.
// Moving each message from one stage of the pipeline to next one is handled by the event loop,
// that runs on a single thread. The expensive CPU-bound computation is handled by the `CpuPool`,
// without blocking the event loop.
pub fn run_async_processor(brokers: &str, group_id: &str, input_topic: &str, warp10_url: &str, warp10_token: &str) {
    // Create the event loop. The event loop will run on a single thread and drive the pipeline.
    let mut core = Core::new().unwrap();

    // Create the CPU pool, for CPU-intensive message processing.
    let cpu_pool = Builder::new().pool_size(4).create();

    let url = warp10_url.to_string();
    let token = warp10_token.to_string();

    // Create the `StreamConsumer`, to receive the messages from the topic in form of a `Stream`.
    let consumer = ClientConfig::new()
        .set("group.id", group_id)
        .set("bootstrap.servers", brokers)
        .set("enable.partition.eof", "false")
        .set("session.timeout.ms", "6000")
        .set("enable.auto.commit", "false")
        .create::<StreamConsumer<_>>()
        .expect("Consumer creation failed");

    consumer.subscribe(&[input_topic]).expect(
        "Can't subscribe to specified topic",
    );


    // Create a handle to the core, that will be used to provide additional asynchronous work
    // to the event loop.
    let handle = core.handle();

    // Create the outer pipeline on the message stream.
    let processed_stream = consumer
        .start()
        .filter_map(|result| {
            // Filter out errors
            match result {
                Ok(msg) => Some(msg),
                Err(kafka_error) => {
                    warn!("Error while receiving from Kafka: {:?}", kafka_error);
                    None
                }
            }
        })
        .for_each(move |msg| {
            // Process each message
            info!("Enqueuing message for computation");
            let owned_message = msg.detach();

            let u = url.clone();
            let t = token.clone();
            // Create the inner pipeline, that represents the processing of a single event.
            let process_message = cpu_pool
                .spawn(lazy(move || {
                    // Take ownership of the message, and runs an expensive computation on it,
                    // using one of the threads of the `cpu_pool`.
                    if let Some(payload) = owned_message.payload() {
                      check_and_post(payload, &u, &t)
                    } else {
                      Err(String::from("no payload"))
                    }
                    //Ok::<_, ()>(())
                }))
                .or_else(|err| {
                    // In case of error, this closure will be executed instead.
                    warn!("Error while processing message: {:?}", err);
                    Ok(())
                });
            // Spawns the inner pipeline in the same event pool.
            handle.spawn(process_message);
            Ok(())
        });

    info!("Starting event loop");
    // Runs the event pool until the consumer terminates.
    core.run(processed_stream).unwrap();
    info!("Stream processing terminated");
}

pub fn send_message(brokers: &str, output_topic: &str, test_url: &str) {
    info!("brokers: {}", brokers);
    // Create the CPU pool, for CPU-intensive message processing.
    //let cpu_pool = Builder::new().pool_size(4).create();
    // Create the `FutureProducer` to produce asynchronously.
    let producer = ClientConfig::new()
        .set("bootstrap.servers", brokers)
        .set("produce.offset.report", "true")
        .create::<FutureProducer<_>>()
        .expect("Producer creation error");

    let topic_name = output_topic.to_string();

    let mut rbe = RequestBenchEvent::default();
    rbe.checks.latency.class_name = String::from("http-latency");
    rbe.checks.status.class_name = String::from("http-status");
    //rbe.labels.insert(String::from("domain"), test_url.to_string());
    rbe.url = test_url.to_string();
    let result = serde_json::to_string(&rbe).unwrap();
    info!("sending\n{}", result);

    producer
        .send_copy::<String, ()>(&topic_name, None, Some(&result), None, None, 1000)
        .and_then(|d_report| {
            // Once the message has been produced, print the delivery report and terminate
            // the pipeline.
            info!("Delivery report for result: {:?}", d_report);
            Ok(())
        })
        .or_else(|err| {
            // In case of error, this closure will be executed instead.
            warn!("Error while processing message: {:?}", err);
            Ok::<_, ()>(())
        });
}
