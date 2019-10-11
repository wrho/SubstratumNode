// Copyright (c) 2017-2018, Substratum LLC (https://substratum.net) and/or its affiliates. All rights reserved.

mod shutdown_supervisor;
pub mod ui_traffic_converter;
mod websocket_supervisor;

use crate::sub_lib::accountant::GetFinancialStatisticsMessage;
use crate::sub_lib::blockchain_bridge::{SetGasPriceMsg, SetWalletPasswordMsg};
use crate::sub_lib::logger::Logger;
use crate::sub_lib::neighborhood::NeighborhoodDotGraphRequest;
use crate::sub_lib::peer_actors::BindMessage;
use crate::sub_lib::ui_gateway::UiGatewaySubs;
use crate::sub_lib::ui_gateway::{FromUiMessage, UiCarrierMessage};
use crate::sub_lib::ui_gateway::{UiGatewayConfig, UiMessage};
use crate::ui_gateway::shutdown_supervisor::ShutdownSupervisor;
use crate::ui_gateway::shutdown_supervisor::ShutdownSupervisorReal;
use crate::ui_gateway::ui_traffic_converter::UiTrafficConverter;
use crate::ui_gateway::ui_traffic_converter::UiTrafficConverterReal;
use crate::ui_gateway::websocket_supervisor::WebSocketSupervisor;
use crate::ui_gateway::websocket_supervisor::WebSocketSupervisorReal;
use actix::Actor;
use actix::Addr;
use actix::Context;
use actix::Handler;
use actix::Recipient;

struct UiGatewayOutSubs {
    ui_message_sub: Recipient<UiCarrierMessage>,
    blockchain_bridge_set_consuming_wallet_password_sub: Recipient<SetWalletPasswordMsg>,
    blockchain_bridge_set_gas_price_sub: Recipient<SetGasPriceMsg>,
    accountant_get_financial_statistics_sub: Recipient<GetFinancialStatisticsMessage>,
    neighborhood: Recipient<NeighborhoodDotGraphRequest>,
}

pub struct UiGateway {
    port: u16,
    node_descriptor: String,
    converter: Box<dyn UiTrafficConverter>,
    subs: Option<UiGatewayOutSubs>,
    websocket_supervisor: Option<Box<dyn WebSocketSupervisor>>,
    shutdown_supervisor: Box<dyn ShutdownSupervisor>,
    logger: Logger,
}

impl UiGateway {
    pub fn new(config: &UiGatewayConfig) -> UiGateway {
        UiGateway {
            port: config.ui_port,
            node_descriptor: config.node_descriptor.clone(),
            converter: Box::new(UiTrafficConverterReal::new()),
            subs: None,
            websocket_supervisor: None,
            shutdown_supervisor: Box::new(ShutdownSupervisorReal::new()),
            logger: Logger::new("UiGateway"),
        }
    }

    pub fn make_subs_from(addr: &Addr<UiGateway>) -> UiGatewaySubs {
        UiGatewaySubs {
            bind: recipient!(addr, BindMessage),
            ui_message_sub: recipient!(addr, UiCarrierMessage),
            from_ui_message_sub: recipient!(addr, FromUiMessage),
        }
    }
}

impl Actor for UiGateway {
    type Context = Context<Self>;
}

impl Handler<BindMessage> for UiGateway {
    type Result = ();

    fn handle(&mut self, msg: BindMessage, _ctx: &mut Self::Context) -> Self::Result {
        //        ctx.set_mailbox_capacity(?);
        let subs = UiGatewayOutSubs {
            ui_message_sub: msg.peer_actors.ui_gateway.ui_message_sub.clone(),
            blockchain_bridge_set_consuming_wallet_password_sub: msg
                .peer_actors
                .blockchain_bridge
                .set_consuming_wallet_password_sub
                .clone(),
            blockchain_bridge_set_gas_price_sub: msg
                .peer_actors
                .blockchain_bridge
                .set_gas_price_sub
                .clone(),
            accountant_get_financial_statistics_sub: msg
                .peer_actors
                .accountant
                .get_financial_statistics_sub
                .clone(),
            neighborhood: msg.peer_actors.neighborhood.from_ui_gateway.clone(),
        };
        self.subs = Some(subs);
        self.websocket_supervisor = Some(Box::new(WebSocketSupervisorReal::new(
            self.port,
            msg.peer_actors.ui_gateway.from_ui_message_sub.clone(),
        )));
        info!(self.logger, "UIGateway bound");
    }
}

impl Handler<UiCarrierMessage> for UiGateway {
    type Result = ();

    // All UI messages, both inbound and outbound, come through here
    fn handle(&mut self, msg: UiCarrierMessage, _ctx: &mut Self::Context) -> Self::Result {
        match msg.data {
            UiMessage::SetWalletPassword(password) => {
                self.subs
                    .as_ref()
                    .expect("UiGateway is unbound")
                    .blockchain_bridge_set_consuming_wallet_password_sub
                    .try_send(SetWalletPasswordMsg {
                        client_id: msg.client_id,
                        password,
                    })
                    .expect("Blockchain Bridge is dead");
            }
            UiMessage::GetFinancialStatisticsMessage => self
                .subs
                .as_ref()
                .expect("UiGateway is unbound")
                .accountant_get_financial_statistics_sub
                .try_send(GetFinancialStatisticsMessage {
                    client_id: msg.client_id,
                })
                .expect("Accountant is dead"),
            UiMessage::ShutdownMessage => {
                info!(self.logger, "Received shutdown order");
                self.shutdown_supervisor.shutdown();
            }
            UiMessage::GetNodeDescriptor => self
                .subs
                .as_ref()
                .expect("UiGateway is unbound")
                .ui_message_sub
                .try_send(UiCarrierMessage {
                    client_id: msg.client_id,
                    data: UiMessage::NodeDescriptor(self.node_descriptor.clone()),
                })
                .expect("UiGateway is dead"),
            UiMessage::SetGasPrice(gas_price) => set_gas_price(self, msg.client_id, &gas_price),
            UiMessage::NodeDescriptor(_)
            | UiMessage::SetWalletPasswordResponse(_)
            | UiMessage::FinancialStatisticsResponse(_)
            | UiMessage::SetGasPriceResponse(_)
            | UiMessage::NeighborhoodDotGraphResponse(_) => {
                let marshalled = self
                    .converter
                    .marshal(msg.data)
                    .expect("Internal error: failed to marshal UiMessage");
                self.websocket_supervisor
                    .as_ref()
                    .expect("WebsocketSupervisor is unbound")
                    .send(msg.client_id, &marshalled);
            }
            UiMessage::NeighborhoodDotGraphRequest => {
                debug!(self.logger, "in UiMessage::NeighborhoodDotGraphRequest");
                self.subs
                    .as_ref()
                    .expect("UiGateway is unbound")
                    .neighborhood
                    .try_send(NeighborhoodDotGraphRequest {
                        client_id: msg.client_id,
                    })
                    .expect("UiGateway is dead");
            }
        }
    }
}

fn set_gas_price(ui_gateway: &UiGateway, client_id: u64, gas_price: &str) {
    ui_gateway
        .subs
        .as_ref()
        .expect("UiGateway is unbound")
        .blockchain_bridge_set_gas_price_sub
        .try_send(SetGasPriceMsg {
            client_id,
            gas_price: gas_price.to_string(),
        })
        .expect("Blockchain Bridge is dead");
}

impl Handler<FromUiMessage> for UiGateway {
    type Result = ();

    // JSON messages from external UIs come in here, are translated to UiMessages, and sent to the handler above
    fn handle(&mut self, msg: FromUiMessage, _ctx: &mut Self::Context) -> Self::Result {
        match self.converter.unmarshal(&msg.json) {
            Err(e) => warning!(
                self.logger,
                "Error unmarshalling message from UI - ignoring: '{}'",
                e
            ),
            Ok(ui_message) => self
                .subs
                .as_ref()
                .expect("UiGateway is unbound")
                .ui_message_sub
                .try_send(UiCarrierMessage {
                    client_id: msg.client_id,
                    data: ui_message,
                })
                .expect("UiGateway is dead"),
        };
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sub_lib::accountant::{FinancialStatisticsMessage, GetFinancialStatisticsMessage};
    use crate::sub_lib::blockchain_bridge::SetWalletPasswordMsg;
    use crate::sub_lib::ui_gateway::UiMessage;
    use crate::test_utils::find_free_port;
    use crate::test_utils::logging::init_test_logging;
    use crate::test_utils::logging::TestLogHandler;
    use crate::test_utils::recorder::peer_actors_builder;
    use crate::test_utils::recorder::{make_recorder, Recorder};
    use crate::test_utils::wait_for;
    use actix::System;
    use std::cell::RefCell;
    use std::sync::Arc;
    use std::sync::Mutex;
    use std::thread;

    impl Default for UiGatewayOutSubs {
        fn default() -> Self {
            let recorder = Recorder::new();
            let addr = recorder.start();
            UiGatewayOutSubs {
                ui_message_sub: addr.clone().recipient::<UiCarrierMessage>(),
                blockchain_bridge_set_consuming_wallet_password_sub: addr
                    .clone()
                    .recipient::<SetWalletPasswordMsg>(
                ),
                blockchain_bridge_set_gas_price_sub: addr.clone().recipient::<SetGasPriceMsg>(),
                accountant_get_financial_statistics_sub: addr
                    .clone()
                    .recipient::<GetFinancialStatisticsMessage>(),
                neighborhood: addr.clone().recipient::<NeighborhoodDotGraphRequest>(),
            }
        }
    }

    pub struct UiTrafficConverterMock {
        marshal_parameters: Arc<Mutex<Vec<UiMessage>>>,
        marshal_results: RefCell<Vec<Result<String, String>>>,
        unmarshal_parameters: Arc<Mutex<Vec<String>>>,
        unmarshal_results: RefCell<Vec<Result<UiMessage, String>>>,
    }

    impl UiTrafficConverter for UiTrafficConverterMock {
        fn marshal(&self, ui_message: UiMessage) -> Result<String, String> {
            self.marshal_parameters.lock().unwrap().push(ui_message);
            self.marshal_results.borrow_mut().remove(0)
        }

        fn unmarshal(&self, json: &str) -> Result<UiMessage, String> {
            self.unmarshal_parameters
                .lock()
                .unwrap()
                .push(String::from(json));
            self.unmarshal_results.borrow_mut().remove(0)
        }
    }

    impl UiTrafficConverterMock {
        fn new() -> UiTrafficConverterMock {
            UiTrafficConverterMock {
                marshal_parameters: Arc::new(Mutex::new(vec![])),
                marshal_results: RefCell::new(vec![]),
                unmarshal_parameters: Arc::new(Mutex::new(vec![])),
                unmarshal_results: RefCell::new(vec![]),
            }
        }

        #[allow(dead_code)]
        fn marshal_parameters(
            mut self,
            parameters: &Arc<Mutex<Vec<UiMessage>>>,
        ) -> UiTrafficConverterMock {
            self.marshal_parameters = parameters.clone();
            self
        }

        #[allow(dead_code)]
        fn marshal_result(self, result: Result<String, String>) -> UiTrafficConverterMock {
            self.marshal_results.borrow_mut().push(result);
            self
        }

        fn unmarshal_parameters(
            mut self,
            parameters: &Arc<Mutex<Vec<String>>>,
        ) -> UiTrafficConverterMock {
            self.unmarshal_parameters = parameters.clone();
            self
        }

        fn unmarshal_result(self, result: Result<UiMessage, String>) -> UiTrafficConverterMock {
            self.unmarshal_results.borrow_mut().push(result);
            self
        }
    }

    #[derive(Default)]
    struct WebSocketSupervisorMock {
        send_parameters: Arc<Mutex<Vec<(u64, String)>>>,
    }

    impl WebSocketSupervisor for WebSocketSupervisorMock {
        fn send(&self, client_id: u64, message_json: &str) {
            self.send_parameters
                .lock()
                .unwrap()
                .push((client_id, String::from(message_json)));
        }
    }

    impl WebSocketSupervisorMock {
        fn new() -> WebSocketSupervisorMock {
            WebSocketSupervisorMock {
                send_parameters: Arc::new(Mutex::new(vec![])),
            }
        }

        fn send_parameters(
            mut self,
            parameters: &Arc<Mutex<Vec<(u64, String)>>>,
        ) -> WebSocketSupervisorMock {
            self.send_parameters = parameters.clone();
            self
        }
    }

    pub struct ShutdownSupervisorMock {
        shutdown_parameters: Arc<Mutex<Vec<()>>>,
    }

    impl ShutdownSupervisor for ShutdownSupervisorMock {
        fn shutdown(&self) {
            self.shutdown_parameters.lock().unwrap().push(());
        }
    }

    impl ShutdownSupervisorMock {
        fn new() -> ShutdownSupervisorMock {
            ShutdownSupervisorMock {
                shutdown_parameters: Arc::new(Mutex::new(vec![])),
            }
        }

        fn shutdown_parameters(
            mut self,
            parameters: &Arc<Mutex<Vec<()>>>,
        ) -> ShutdownSupervisorMock {
            self.shutdown_parameters = parameters.clone();
            self
        }
    }

    #[test]
    fn receiving_a_get_financial_statistics_message_sends_traffic_to_the_accountant() {
        let (accountant, _, accountant_recording_arc) = make_recorder();
        let subject = UiGateway::new(&UiGatewayConfig {
            ui_port: find_free_port(),
            node_descriptor: String::from(""),
        });
        let system = System::new(
            "receiving_a_get_financial_statistics_message_sends_traffic_to_the_accountant",
        );
        let addr: Addr<UiGateway> = subject.start();
        let mut peer_actors = peer_actors_builder().accountant(accountant).build();
        peer_actors.ui_gateway = UiGateway::make_subs_from(&addr);
        addr.try_send(BindMessage { peer_actors }).unwrap();

        addr.try_send(UiCarrierMessage {
            client_id: 3,
            data: UiMessage::GetFinancialStatisticsMessage,
        })
        .unwrap();

        System::current().stop();
        system.run();

        let accountant_recorder = accountant_recording_arc.lock().unwrap();
        assert_eq!(
            accountant_recorder.get_record::<GetFinancialStatisticsMessage>(0),
            &GetFinancialStatisticsMessage { client_id: 3 }
        )
    }

    #[test]
    fn receiving_a_set_consuming_wallet_password_message_sends_traffic_to_blockchain_bridge() {
        let (blockchain_bridge, _, blockchain_bridge_recorder_arc) = make_recorder();
        let subject = UiGateway::new(&UiGatewayConfig {
            ui_port: find_free_port(),
            node_descriptor: String::from(""),
        });
        let system = System::new(
            "receiving_a_set_consuming_wallet_password_message_sends_traffic_to_blockchain_bridge",
        );
        let addr: Addr<UiGateway> = subject.start();
        let mut peer_actors = peer_actors_builder()
            .blockchain_bridge(blockchain_bridge)
            .build();
        peer_actors.ui_gateway = UiGateway::make_subs_from(&addr);
        addr.try_send(BindMessage { peer_actors }).unwrap();

        addr.try_send(UiCarrierMessage {
            client_id: 0,
            data: UiMessage::SetWalletPassword("booga".to_string()),
        })
        .unwrap();

        System::current().stop();
        system.run();
        let blockchain_bridge_recorder = blockchain_bridge_recorder_arc.lock().unwrap();
        assert_eq!(
            blockchain_bridge_recorder.get_record::<SetWalletPasswordMsg>(0),
            &SetWalletPasswordMsg {
                client_id: 0,
                password: "booga".to_string(),
            }
        )
    }

    #[test]
    fn receiving_a_shutdown_message_triggers_the_shutdown_supervisor() {
        let shutdown_parameters = Arc::new(Mutex::new(vec![]));
        let shutdown_parameters_inside = shutdown_parameters.clone();

        thread::spawn(move || {
            let supervisor =
                ShutdownSupervisorMock::new().shutdown_parameters(&shutdown_parameters_inside);
            let mut subject = UiGateway::new(&UiGatewayConfig {
                ui_port: find_free_port(),
                node_descriptor: String::from(""),
            });
            subject.shutdown_supervisor = Box::new(supervisor);
            let system =
                System::new("receiving_a_shutdown_message_triggers_the_shutdown_supervisor");
            let addr: Addr<UiGateway> = subject.start();
            let mut peer_actors = peer_actors_builder().build();
            peer_actors.ui_gateway = UiGateway::make_subs_from(&addr);
            addr.try_send(BindMessage { peer_actors }).unwrap();

            addr.try_send(UiCarrierMessage {
                client_id: 0,
                data: UiMessage::ShutdownMessage,
            })
            .unwrap();

            system.run();
        });
        wait_for(None, None, || shutdown_parameters.lock().unwrap().len() > 0)
    }

    #[test]
    fn receiving_a_get_node_descriptor_message_triggers_a_node_descriptor_response() {
        let (ui_gateway_recorder, ui_gateway_awaiter, ui_gateway_recording_arc) = make_recorder();

        thread::spawn(move || {
            let system = System::new(
                "receiving_a_get_node_descriptor_message_triggers_a_node_descriptor_response",
            );
            let mut subject = UiGateway::new(&UiGatewayConfig {
                ui_port: find_free_port(),
                node_descriptor: String::from("NODE-DESCRIPTOR"),
            });
            let ui_gateway_recorder_addr = ui_gateway_recorder.start();
            subject.subs = Some(UiGatewayOutSubs {
                ui_message_sub: ui_gateway_recorder_addr.recipient::<UiCarrierMessage>(),
                ..Default::default()
            });
            let subject_addr = subject.start();
            let subject_subs = UiGateway::make_subs_from(&subject_addr);

            let request = serde_json::to_string(&UiMessage::GetNodeDescriptor).unwrap();
            subject_subs
                .from_ui_message_sub
                .try_send(FromUiMessage {
                    client_id: 1234,
                    json: request,
                })
                .unwrap();

            subject_subs
                .ui_message_sub
                .try_send(UiCarrierMessage {
                    client_id: 1234,
                    data: UiMessage::GetNodeDescriptor,
                })
                .unwrap();

            system.run();
        });

        ui_gateway_awaiter.await_message_count(2);

        let ui_gateway_recording = ui_gateway_recording_arc.lock().unwrap();
        assert_eq!(ui_gateway_recording.len(), 2);
        assert_eq!(
            ui_gateway_recording.get_record::<UiCarrierMessage>(1),
            &UiCarrierMessage {
                client_id: 1234,
                data: UiMessage::NodeDescriptor("NODE-DESCRIPTOR".to_string())
            }
        );
    }

    #[test]
    fn node_descriptor_message_is_directed_to_websocket_supervisor() {
        let (ui_gateway_recorder, _, _) = make_recorder();
        let receive_parameters_arc = Arc::new(Mutex::new(vec![]));

        let system = System::new("node_descriptor_message_is_directed_to_websocket_supervisor");
        let mut subject = UiGateway::new(&UiGatewayConfig {
            ui_port: find_free_port(),
            node_descriptor: String::from(""),
        });
        subject.websocket_supervisor = Some(Box::new(
            WebSocketSupervisorMock::new().send_parameters(&receive_parameters_arc),
        ));
        let ui_gateway_recorder_addr = ui_gateway_recorder.start();
        subject.subs = Some(UiGatewayOutSubs {
            ui_message_sub: ui_gateway_recorder_addr.recipient::<UiCarrierMessage>(),
            ..Default::default()
        });
        let subject_addr = subject.start();
        let subject_subs = UiGateway::make_subs_from(&subject_addr);

        subject_subs
            .ui_message_sub
            .try_send(UiCarrierMessage {
                client_id: 1234,
                data: UiMessage::NodeDescriptor("NODE-DESCRIPTOR".to_string()),
            })
            .unwrap();

        System::current().stop();
        system.run();

        wait_for(None, None, || {
            receive_parameters_arc.lock().unwrap().len() > 0
        });
        assert_eq!(
            receive_parameters_arc
                .clone()
                .lock()
                .unwrap()
                .get(0)
                .unwrap(),
            &(
                1234 as u64,
                serde_json::to_string(&UiMessage::NodeDescriptor("NODE-DESCRIPTOR".to_string()))
                    .unwrap()
            )
        )
    }

    #[test]
    fn set_consuming_wallet_password_response_message_is_directed_to_websocket_supervisor() {
        let (ui_gateway_recorder, _, _) = make_recorder();
        let receive_parameters_arc = Arc::new(Mutex::new(vec![]));

        let system = System::new(
            "set_consuming_wallet_password_response_message_is_directed_to_websocket_supervisor",
        );
        let mut subject = UiGateway::new(&UiGatewayConfig {
            ui_port: find_free_port(),
            node_descriptor: String::from(""),
        });
        subject.websocket_supervisor = Some(Box::new(
            WebSocketSupervisorMock::new().send_parameters(&receive_parameters_arc),
        ));
        let ui_gateway_recorder_addr = ui_gateway_recorder.start();
        subject.subs = Some(UiGatewayOutSubs {
            ui_message_sub: ui_gateway_recorder_addr.recipient::<UiCarrierMessage>(),
            ..Default::default()
        });
        let subject_addr = subject.start();
        let subject_subs = UiGateway::make_subs_from(&subject_addr);

        subject_subs
            .ui_message_sub
            .try_send(UiCarrierMessage {
                client_id: 1234,
                data: UiMessage::SetWalletPasswordResponse(true),
            })
            .unwrap();

        System::current().stop();
        system.run();

        wait_for(None, None, || {
            receive_parameters_arc.lock().unwrap().len() > 0
        });
        assert_eq!(
            receive_parameters_arc
                .clone()
                .lock()
                .unwrap()
                .get(0)
                .unwrap(),
            &(
                1234 as u64,
                serde_json::to_string(&UiMessage::SetWalletPasswordResponse(true)).unwrap()
            )
        )
    }

    #[test]
    fn receiving_a_set_gas_price_message_sends_traffic_to_blockchain_bridge() {
        let (blockchain_bridge, _, blockchain_bridge_recorder_arc) = make_recorder();
        let subject = UiGateway::new(&UiGatewayConfig {
            ui_port: find_free_port(),
            node_descriptor: String::from(""),
        });
        let system =
            System::new("receiving_a_set_gas_price_message_sends_traffic_to_blockchain_bridge");
        let addr: Addr<UiGateway> = subject.start();
        let mut peer_actors = peer_actors_builder()
            .blockchain_bridge(blockchain_bridge)
            .build();
        peer_actors.ui_gateway = UiGateway::make_subs_from(&addr);
        addr.try_send(BindMessage { peer_actors }).unwrap();

        addr.try_send(UiCarrierMessage {
            client_id: 0,
            data: UiMessage::SetGasPrice("11".to_string()),
        })
        .unwrap();

        System::current().stop();
        system.run();
        let blockchain_bridge_recorder = blockchain_bridge_recorder_arc.lock().unwrap();
        assert_eq!(
            blockchain_bridge_recorder.get_record::<SetGasPriceMsg>(0),
            &SetGasPriceMsg {
                client_id: 0,
                gas_price: "11".to_string(),
            }
        )
    }

    #[test]
    fn set_gas_price_response_message_is_directed_to_websocket_supervisor() {
        let (ui_gateway_recorder, _, _) = make_recorder();
        let receive_parameters_arc = Arc::new(Mutex::new(vec![]));

        let system =
            System::new("set_gas_price_response_message_is_directed_to_websocket_supervisor");
        let mut subject = UiGateway::new(&UiGatewayConfig {
            ui_port: find_free_port(),
            node_descriptor: "".to_string(),
        });
        subject.websocket_supervisor = Some(Box::new(
            WebSocketSupervisorMock::new().send_parameters(&receive_parameters_arc),
        ));
        let ui_gateway_recorder_addr = ui_gateway_recorder.start();
        subject.subs = Some(UiGatewayOutSubs {
            ui_message_sub: ui_gateway_recorder_addr.recipient::<UiCarrierMessage>(),
            ..Default::default()
        });
        let subject_addr = subject.start();
        let subject_subs = UiGateway::make_subs_from(&subject_addr);

        subject_subs
            .ui_message_sub
            .try_send(UiCarrierMessage {
                client_id: 1234,
                data: UiMessage::SetGasPriceResponse(true),
            })
            .unwrap();

        System::current().stop();
        system.run();

        wait_for(None, None, || {
            receive_parameters_arc.lock().unwrap().len() > 0
        });
        assert_eq!(
            receive_parameters_arc
                .clone()
                .lock()
                .unwrap()
                .get(0)
                .unwrap(),
            &(
                1234 as u64,
                serde_json::to_string(&UiMessage::SetGasPriceResponse(true)).unwrap()
            )
        )
    }

    #[test]
    fn financial_statistics_response_message_is_directed_to_websocket_supervisor() {
        let (ui_gateway_recorder, _, _) = make_recorder();
        let receive_parameters_arc = Arc::new(Mutex::new(vec![]));

        let system = System::new(
            "financial_statistics_response_message_is_directed_to_websocket_supervisor",
        );
        let mut subject = UiGateway::new(&UiGatewayConfig {
            ui_port: find_free_port(),
            node_descriptor: String::from(""),
        });
        subject.websocket_supervisor = Some(Box::new(
            WebSocketSupervisorMock::new().send_parameters(&receive_parameters_arc),
        ));
        let ui_gateway_recorder_addr = ui_gateway_recorder.start();
        subject.subs = Some(UiGatewayOutSubs {
            ui_message_sub: ui_gateway_recorder_addr.recipient::<UiCarrierMessage>(),
            ..Default::default()
        });
        let subject_addr = subject.start();
        let subject_subs = UiGateway::make_subs_from(&subject_addr);

        subject_subs
            .ui_message_sub
            .try_send(UiCarrierMessage {
                client_id: 1234,
                data: UiMessage::FinancialStatisticsResponse(FinancialStatisticsMessage {
                    pending_credit: 1_000_000_001,
                    pending_debt: 2_000_000_001,
                }),
            })
            .unwrap();

        System::current().stop();
        system.run();

        wait_for(None, None, || {
            receive_parameters_arc.lock().unwrap().len() > 0
        });
        assert_eq!(
            receive_parameters_arc
                .clone()
                .lock()
                .unwrap()
                .get(0)
                .unwrap(),
            &(
                1234 as u64,
                serde_json::to_string(&UiMessage::FinancialStatisticsResponse(
                    FinancialStatisticsMessage {
                        pending_credit: 1_000_000_001,
                        pending_debt: 2_000_000_001
                    }
                ))
                .unwrap()
            )
        )
    }

    #[test]
    fn good_from_ui_message_is_unmarshalled_and_resent() {
        let unmarshal_parameters = Arc::new(Mutex::new(vec![]));
        let handler = UiTrafficConverterMock::new()
            .unmarshal_parameters(&unmarshal_parameters)
            .unmarshal_result(Ok(UiMessage::ShutdownMessage));
        let (ui_gateway, ui_gateway_awaiter, ui_gateway_recording_arc) = make_recorder();

        thread::spawn(move || {
            let mut subject = UiGateway::new(&UiGatewayConfig {
                ui_port: find_free_port(),
                node_descriptor: String::from(""),
            });
            subject.converter = Box::new(handler);
            let system = System::new("good_from_ui_message_is_unmarshalled_and_resent");
            let addr: Addr<UiGateway> = subject.start();
            let peer_actors = peer_actors_builder().ui_gateway(ui_gateway).build();
            addr.try_send(BindMessage { peer_actors }).unwrap();

            addr.try_send(FromUiMessage {
                client_id: 42,
                json: String::from("pretend I'm JSON"),
            })
            .unwrap();

            system.run();
        });
        ui_gateway_awaiter.await_message_count(1);
        let unmarshal_parameters_locked = unmarshal_parameters.lock().unwrap();
        assert_eq!(
            unmarshal_parameters_locked[0],
            String::from("pretend I'm JSON")
        );
        assert_eq!(unmarshal_parameters_locked.len(), 1);
        let ui_gateway_recording = ui_gateway_recording_arc.lock().unwrap();
        assert_eq!(
            ui_gateway_recording.get_record::<UiCarrierMessage>(0),
            &UiCarrierMessage {
                client_id: 42,
                data: UiMessage::ShutdownMessage
            }
        );
    }

    #[test]
    fn bad_from_ui_message_is_logged_and_ignored() {
        init_test_logging();
        let handler =
            UiTrafficConverterMock::new().unmarshal_result(Err(String::from("I have a tummyache")));
        let (ui_gateway, _, ui_gateway_recording_arc) = make_recorder();

        thread::spawn(move || {
            let mut subject = UiGateway::new(&UiGatewayConfig {
                ui_port: find_free_port(),
                node_descriptor: String::from(""),
            });
            subject.converter = Box::new(handler);
            let system = System::new("bad_from_ui_message_is_logged_and_ignored");
            let addr: Addr<UiGateway> = subject.start();
            let peer_actors = peer_actors_builder().ui_gateway(ui_gateway).build();
            addr.try_send(BindMessage { peer_actors }).unwrap();

            addr.try_send(FromUiMessage {
                client_id: 0,
                json: String::from("pretend I'm JSON"),
            })
            .unwrap();

            system.run();
        });
        TestLogHandler::new().await_log_containing(
            "Error unmarshalling message from UI - ignoring: 'I have a tummyache'",
            1000,
        );
        let ui_gateway_recording = ui_gateway_recording_arc.lock().unwrap();
        assert_eq!(ui_gateway_recording.len(), 0);
    }

    #[test]
    fn request_for_dot_graph_forwards_request_to_neighbor() {
        let (neighborhood, _, neighborhood_recorder_arc) = make_recorder();
        let subject = UiGateway::new(&UiGatewayConfig {
            ui_port: find_free_port(),
            node_descriptor: String::from(""),
        });
        let system = System::new("request_for_dot_graph_forwards_request_to_neighbor");
        let addr: Addr<UiGateway> = subject.start();
        let mut peer_actors = peer_actors_builder().neighborhood(neighborhood).build();
        peer_actors.ui_gateway = UiGateway::make_subs_from(&addr);
        addr.try_send(BindMessage { peer_actors }).unwrap();

        let json = UiTrafficConverterReal::new()
            .marshal(UiMessage::NeighborhoodDotGraphRequest)
            .unwrap();
        addr.try_send(FromUiMessage { client_id: 0, json }).unwrap();

        System::current().stop();
        system.run();
        let neighborhood_recorder = neighborhood_recorder_arc.lock().unwrap();
        let actual_request = neighborhood_recorder.get_record::<NeighborhoodDotGraphRequest>(0);
        assert_eq!(
            actual_request,
            &NeighborhoodDotGraphRequest { client_id: 0 }
        );
    }
}
