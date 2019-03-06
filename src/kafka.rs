use std::thread;

use futures::Future;
use futures::future::lazy;
use futures::Stream;
use futures_cpupool::Builder;
use lazy_static::lazy_static;
use prometheus::{Counter, Encoder, opts, register_counter, TextEncoder};
use rdkafka::config::ClientConfig;
use rdkafka::consumer::Consumer;
use rdkafka::consumer::stream_consumer::StreamConsumer;
use rdkafka::Message;
use rdkafka::producer::{FutureProducer, FutureRecord};
use serde_json;
use time;
use tokio::executor::current_thread::CurrentThread;
use warp::{Filter, path, serve};
use warp10;
use warp10::Label;

use BufferedDomainTestResult;
use check::run_check_for_url;
use RequestBenchEvent;
use warp10_post;

lazy_static! {
    static ref KAFKA_ERROR_COUNTER: Counter = register_counter!(opts!(
        "http_agent_kafka_error",
        "Number of kafka errors"
    ))
    .expect("to create the http_agent_kafka_error counter");

    static ref CHECK_ERROR_COUNTER: Counter = register_counter!(opts!(
        "http_agent_check_error",
        "Number of check errors"
    ))
    .expect("to create the http_agent_check_error counter");

    static ref CHECK_COUNTER: Counter = register_counter!(opts!(
        "http_agent_check",
        "Number of check errors"
    ))
    .expect("to create the http_agent_check counter");
}

//FIXME: send back an error
fn check_and_post(payload: &[u8], host: &str, zone: &str) -> Result<(), String> {
    let r: serde_json::Result<RequestBenchEvent> = serde_json::from_slice(payload);
    match r {
        Ok(request) => {
            debug!("got request: {:#?}", request);
            let check = run_check_for_url(&request.url, true);
            let labels = vec![
                Label::new("host", host),
                Label::new("zone", zone),
            ];

            let endpoint = request.warp10_endpoint.clone();
            let token = request.token.clone();

            let result = BufferedDomainTestResult {
                domain_test_results: vec![check],
                timestamp: time::now_utc().to_timespec(),
                request_bench_event: request,
                labels,
            };

            let mut data: Vec<warp10::Data> = result.into();

            info!("sending to warp10: {:?}", data);

            let res = warp10_post(data, endpoint, token);
            info!("result sending to warp10: {:#?}", res);
            match res {
                Ok(_) => Ok(()),
                Err(e) => Err(format!("{:?}", e)),
            }
        }
        Err(e) => Err(format!("{:?}", e)),
    }
}

pub fn run_async_processor(brokers: &str, group_id: &str, input_topic: &str, username: Option<String>, password: Option<String>, host: String, zone: String) {
    let mut io_loop = CurrentThread::new();

    let cpu_pool = Builder::new().pool_size(4).create();

    let mut consumer = ClientConfig::new();

    if let (Some(user), Some(pass)) = (username, password) {
        consumer
            .set("security.protocol", "SASL_SSL")
            .set("sasl.mechanisms", "PLAIN")
            .set("sasl.username", &user)
            .set("sasl.password", &pass);
    }

    let consumer = consumer
        .set("group.id", group_id)
        .set("bootstrap.servers", brokers)
        .set("enable.partition.eof", "false")
        .set("session.timeout.ms", "6000")
        .set("enable.auto.commit", "true")
        .create::<StreamConsumer<_>>()
        .expect("Consumer creation failed");

    consumer.subscribe(&[input_topic]).expect(
        "Can't subscribe to specified topic",
    );

    let handle = io_loop.handle();

    let processed_stream = consumer
        .start()
        .filter_map(|result| {
            match result {
                Ok(msg) => Some(msg),
                Err(kafka_error) => {
                    KAFKA_ERROR_COUNTER.inc();
                    warn!("Error while receiving from Kafka: {:?}", kafka_error);
                    None
                }
            }
        })
        .for_each(move |msg| {
            info!("Enqueuing message for computation");
            let owned_message = msg.detach();

            let h = host.clone();
            let z = zone.clone();
            let process_message = cpu_pool
                .spawn(lazy(move || {
                    CHECK_COUNTER.inc();
                    if let Some(payload) = owned_message.payload() {
                        check_and_post(payload, &h, &z)
                    } else {
                        Err(String::from("no payload"))
                    }
                }))
                .or_else(|err| {
                    CHECK_ERROR_COUNTER.inc();
                    warn!("Error while processing message: {:?}", err);
                    Ok(())
                });
            handle.spawn(process_message).expect("to not fail to process a message");
            Ok(())
        });

    let metrics = path!("metrics")
        .map(|| {
            let encoder = TextEncoder::new();
            let metric_families = prometheus::gather();
            let mut buffer = vec![];
            encoder.encode(&metric_families, &mut buffer).unwrap();
            buffer
        });

    let metrics_thread = thread::spawn(move || {
        serve(metrics)
            .run(([127, 0, 0, 1], 3030));
    });

    info!("Starting event loop");
    io_loop.block_on(processed_stream).unwrap();
    info!("Stream processing terminated");

    metrics_thread.join().expect("to join the metrics server thread");
}

pub fn send_message(brokers: &str, output_topic: &str, domain_name: &str, warp10_endpoint: &str, token: &str, test_url: &str, username: Option<String>, password: Option<String>) {
    let mut producer = ClientConfig::new();

    if let (Some(user), Some(pass)) = (username, password) {
        producer
            .set("security.protocol", "SASL_SSL")
            .set("sasl.mechanisms", "PLAIN")
            .set("sasl.username", &user)
            .set("sasl.password", &pass);
    }

    let producer = producer
        .set("bootstrap.servers", brokers)
        .set("produce.offset.report", "true")
        .create::<FutureProducer<_>>()
        .expect("Producer creation error");

    let topic_name = output_topic.to_string();

    let mut rbe = RequestBenchEvent::default();
    rbe.domain_name = domain_name.to_string();
    rbe.warp10_endpoint = warp10_endpoint.to_string();
    rbe.token = token.to_string();
    rbe.url = test_url.to_string();
    let result = serde_json::to_string(&rbe).unwrap();
    info!("sending\n{}", result);

    producer
        .send::<(), String>(FutureRecord::to(&topic_name).payload(&result), 1000)
        .and_then(|d_report| {
            info!("Delivery report for result: {:?}", d_report);
            Ok(())
        })
        .or_else(|err| {
            warn!("Error while processing message: {:?}", err);
            Ok::<_, ()>(())
        }).wait().unwrap();
}
