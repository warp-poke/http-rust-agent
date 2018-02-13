use futures::Stream;
use futures::Future;
use futures::future::lazy;

use futures_cpupool::Builder;
use tokio_core::reactor::Core;
use warp10;

use rdkafka::Message;
use rdkafka::consumer::Consumer;
use rdkafka::consumer::stream_consumer::StreamConsumer;
use rdkafka::config::ClientConfig;
use rdkafka::message::OwnedMessage;
use rdkafka::producer::FutureProducer;

use std::thread;
use std::time::Duration;

use time;
use serde_json;

use check::run_check_for_url;
use warp10_post;
use BufferedDomainTestResult;
use RequestBenchEvent;

//FIXME: send back an error
fn expensive_computation(msg: OwnedMessage, warp10_url: &str, warp10_token: &str) {
    info!("Starting expensive computation on message");
    thread::sleep(Duration::from_millis(5000));
    info!("Expensive computation completed");
    if let Some(payload) = msg.payload() {
      let request: RequestBenchEvent = serde_json::from_slice(payload).unwrap();

      let check = run_check_for_url(&request.url, true);

      let result = BufferedDomainTestResult {
        domain_test_results: vec![check],
        timestamp: time::now_utc().to_timespec(),
        //FIXME: remove
        delivery_tag: 42,
        request_bench_event: request
      };

      let data: Vec<warp10::Data> = result.into();

      let res = warp10_post(data, warp10_url.to_string(), warp10_token.to_string());
      println!("{:#?}", res);
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
fn run_async_processor(brokers: &str, group_id: &str, input_topic: &str, output_topic: &str,  warp10_url: &str, warp10_token: &str) {
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

    consumer.subscribe(&[input_topic]).expect("Can't subscribe to specified topic");


    // Create a handle to the core, that will be used to provide additional asynchronous work
    // to the event loop.
    let handle = core.handle();

    // Create the outer pipeline on the message stream.
    let processed_stream = consumer.start()
        .filter_map(|result| {  // Filter out errors
            match result {
                Ok(msg) => Some(msg),
                Err(kafka_error) => {
                    warn!("Error while receiving from Kafka: {:?}", kafka_error);
                    None
                }
            }
        }).for_each(move |msg| {     // Process each message
            info!("Enqueuing message for computation");
            let owned_message = msg.detach();

            let u = url.clone();
            let t = token.clone();
            // Create the inner pipeline, that represents the processing of a single event.
            let process_message = cpu_pool.spawn(lazy(move || {
                let url = u.clone();
                let token = t.clone();
                // Take ownership of the message, and runs an expensive computation on it,
                // using one of the threads of the `cpu_pool`.
                expensive_computation(owned_message, &u, &t);
                Ok::<_, ()>(())
            })).or_else(|err| {
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

fn send_message(brokers: &str, output_topic: &str, test_url: &str) {
    // Create the event loop. The event loop will run on a single thread and drive the pipeline.
    let mut core = Core::new().unwrap();

    // Create the CPU pool, for CPU-intensive message processing.
    //let cpu_pool = Builder::new().pool_size(4).create();
    // Create the `FutureProducer` to produce asynchronously.
    let producer = ClientConfig::new()
        .set("bootstrap.servers", brokers)
        .set("produce.offset.report", "true")
        .create::<FutureProducer<_>>()
        .expect("Producer creation error");

    let handle = core.handle();
    let topic_name = output_topic.to_string();
    // Create the inner pipeline, that represents the processing of a single event.
    /*let send = cpu_pool.spawn_fn(move || {
      info!("Sending result");
      let result = String::from("a");
      let topic = topic_name.clone();
      producer.send_copy::<String, ()>(&topic_name, None, Some(&result), None, None, 1000)
    })*/
    let result = String::from("a");
    producer.send_copy::<String, ()>(&topic_name, None, Some(&result), None, None, 1000)
    .and_then(|d_report| {
      // Once the message has been produced, print the delivery report and terminate
      // the pipeline.
      info!("Delivery report for result: {:?}", d_report);
      Ok(())
    }).or_else(|err| {
      // In case of error, this closure will be executed instead.
      warn!("Error while processing message: {:?}", err);
      Ok::<_, ()>(())
    });
    // Spawns the inner pipeline in the same event pool.
    //handle.spawn(send);
}

