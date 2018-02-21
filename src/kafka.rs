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
use warp10::Label;

use std::collections::HashMap;

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
        },
        Err(e) => Err(format!("{:?}", e)),

    }
}

pub fn run_async_processor(brokers: &str, group_id: &str, input_topic: &str, username: Option<String>, password: Option<String>, host: String, zone: String) {
    let mut core = Core::new().unwrap();

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

    let handle = core.handle();

    let processed_stream = consumer
        .start()
        .filter_map(|result| {
            match result {
                Ok(msg) => Some(msg),
                Err(kafka_error) => {
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

                    if let Some(payload) = owned_message.payload() {
                      check_and_post(payload, &h, &z)
                    } else {
                      Err(String::from("no payload"))
                    }
                }))
                .or_else(|err| {
                    warn!("Error while processing message: {:?}", err);
                    Ok(())
                });
            handle.spawn(process_message);
            Ok(())
        });

    info!("Starting event loop");
    core.run(processed_stream).unwrap();
    info!("Stream processing terminated");
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
        .send_copy::<String, ()>(&topic_name, None, Some(&result), None, None, 1000)
        .and_then(|d_report| {
            info!("Delivery report for result: {:?}", d_report);
            Ok(())
        })
        .or_else(|err| {
            warn!("Error while processing message: {:?}", err);
            Ok::<_, ()>(())
        }).wait().unwrap();
}
