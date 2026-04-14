use crate::geoip::{GeoIPService, GeoLocation, format_location_for_display, get_country_flag};
use crate::traceroute::{IpVersion, ProductionTraceroute, TracerouteResult};
use iced::alignment::Vertical;
use iced::widget::{
    Column, Container, button, checkbox, column, container, row, scrollable, text, text_input,
    vertical_space,
};
use iced::{Alignment, Application, Command, Element, Font, Length, Renderer, Theme};
use log::info;
use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use tokio::sync::mpsc;

pub struct TracerouteApp {
    target_input: String,
    is_tracing: bool,
    trace_results: Vec<TracerouteResult>,
    current_hops: Vec<crate::traceroute::HopInfo>,
    max_hops: u32,
    timeout_ms: u64,
    ip_version: IpVersion,
    geo_locations: HashMap<String, GeoLocation>,
    geo_service: GeoIPService,
    command_tx: Option<mpsc::UnboundedSender<TraceCommand>>,
    result_rx: Option<mpsc::UnboundedReceiver<TraceResult>>,
    geo_tx: Option<mpsc::UnboundedSender<(String, GeoLocation)>>,
    geo_rx: Option<mpsc::UnboundedReceiver<(String, GeoLocation)>>,
    status_message: String,
    cancel_flag: Option<Arc<AtomicBool>>,
}

#[derive(Debug, Clone)]
pub enum Message {
    TargetInputChanged(String),
    StartTrace,
    StopTrace,
    ClearResults,
    MaxHopsChanged(u32),
    TimeoutChanged(u64),
    IpVersionChanged(IpVersion),
    CheckAsyncResults,
}

#[derive(Debug, Clone)]
pub enum TraceCommand {
    StopTrace,
}

#[derive(Debug, Clone)]
pub enum TraceResult {
    HopUpdate(crate::traceroute::HopInfo),
    Completed(TracerouteResult),
    Error(String),
}

impl Default for TracerouteApp {
    fn default() -> Self {
        Self::new()
    }
}

impl TracerouteApp {
    pub fn new() -> Self {
        let (geo_tx, geo_rx) = mpsc::unbounded_channel();

        Self {
            target_input: String::new(),
            is_tracing: false,
            trace_results: Vec::new(),
            current_hops: Vec::new(),
            max_hops: 30,
            timeout_ms: 5000,
            ip_version: IpVersion::IPv4,
            geo_locations: HashMap::new(),
            geo_service: GeoIPService::new(),
            command_tx: None,
            result_rx: None,
            geo_tx: Some(geo_tx),
            geo_rx: Some(geo_rx),
            status_message: "就绪".to_string(),
            cancel_flag: None,
        }
    }

    fn start_trace(&mut self) -> Command<Message> {
        if self.target_input.trim().is_empty() {
            self.status_message = "请输入目标地址".to_string();
            return Command::none();
        }

        self.is_tracing = true;
        self.current_hops.clear();
        self.status_message = format!("正在追踪: {}", self.target_input);

        let cancel_flag = Arc::new(AtomicBool::new(false));
        self.cancel_flag = Some(cancel_flag.clone());

        let (command_tx, _command_rx) = mpsc::unbounded_channel::<TraceCommand>();
        let (result_tx, result_rx) = mpsc::unbounded_channel::<TraceResult>();

        self.command_tx = Some(command_tx);
        self.result_rx = Some(result_rx);

        let target = self.target_input.clone();
        let max_hops = self.max_hops;
        let timeout_ms = self.timeout_ms;
        let ip_version = self.ip_version.clone();
        let result_tx_clone = result_tx.clone();
        let cancel_flag_clone = cancel_flag.clone();

        // 启动追踪线程
        std::thread::spawn(move || {
            let rt = tokio::runtime::Runtime::new().unwrap();
            rt.block_on(async {
                let mut tracer = ProductionTraceroute::with_config(
                    max_hops,
                    timeout_ms,
                    ip_version,
                );

                let hop_result_tx = result_tx_clone.clone();
                let complete_result_tx = result_tx_clone.clone();

                let callback = move |hop: crate::traceroute::HopInfo| {
                    let _ = hop_result_tx.send(TraceResult::HopUpdate(hop));
                };

                match tracer
                    .trace_with_callback(&target, callback, cancel_flag_clone)
                    .await
                {
                    Ok(result) => {
                        let _ = complete_result_tx.send(TraceResult::Completed(result));
                    }
                    Err(e) => {
                        let _ = result_tx_clone.send(TraceResult::Error(e.to_string()));
                    }
                }
            });
        });

        Command::perform(
            async {
                tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
            },
            |_| Message::CheckAsyncResults,
        )
    }

    fn stop_trace(&mut self) {
        self.is_tracing = false;
        if let Some(ref tx) = self.command_tx {
            let _ = tx.send(TraceCommand::StopTrace);
        }
        if let Some(ref cancel_flag) = self.cancel_flag {
            cancel_flag.store(true, Ordering::Relaxed);
        }
        self.status_message = "已停止".to_string();
    }

    fn check_results(&mut self) -> Command<Message> {
        // 处理地理位置更新
        if let Some(ref mut geo_rx) = self.geo_rx {
            while let Ok((ip, location)) = geo_rx.try_recv() {
                self.geo_locations.insert(ip, location);
            }
        }

        // 处理追踪结果
        if let Some(ref mut rx) = self.result_rx {
            while let Ok(result) = rx.try_recv() {
                match result {
                    TraceResult::HopUpdate(hop) => {
                        info!("Hop {}: {}", hop.hop, hop.ip);
                        
                        // 更新或添加跳点信息
                        if let Some(index) = self.current_hops.iter().position(|h| h.hop == hop.hop) {
                            self.current_hops[index] = hop.clone();
                        } else {
                            self.current_hops.push(hop.clone());
                        }

                        // 异步查询地理位置
                        if !self.geo_locations.contains_key(&hop.ip) && hop.ip != "*" {
                            let geo_tx = self.geo_tx.as_ref().unwrap().clone();
                            let mut geo_service = self.geo_service.clone();
                            let ip = hop.ip.clone();

                            std::thread::spawn(move || {
                                let rt = tokio::runtime::Runtime::new().unwrap();
                                rt.block_on(async {
                                    if let Ok(location) = geo_service.get_location(&ip).await {
                                        let _ = geo_tx.send((ip, location));
                                    }
                                });
                            });
                        }
                    }
                    TraceResult::Completed(result) => {
                        self.is_tracing = false;
                        self.trace_results.push(result.clone());
                        self.status_message = "追踪完成".to_string();
                    }
                    TraceResult::Error(err) => {
                        self.is_tracing = false;
                        self.status_message = format!("错误: {}", err);
                    }
                }
            }
        }

        Command::perform(
            async {
                tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
            },
            |_| Message::CheckAsyncResults,
        )
    }
}

impl Application for TracerouteApp {
    type Executor = iced::executor::Default;
    type Message = Message;
    type Theme = Theme;
    type Flags = ();

    fn new(_flags: ()) -> (Self, Command<Message>) {
        (Self::new(), Command::none())
    }

    fn title(&self) -> String {
        "网络追踪工具".to_string()
    }

    fn update(&mut self, message: Message) -> Command<Message> {
        match message {
            Message::TargetInputChanged(input) => {
                self.target_input = input;
                Command::none()
            }
            Message::StartTrace => self.start_trace(),
            Message::StopTrace => {
                self.stop_trace();
                Command::none()
            }
            Message::ClearResults => {
                self.trace_results.clear();
                self.current_hops.clear();
                self.geo_locations.clear();
                self.status_message = "已清理".to_string();
                Command::none()
            }
            Message::MaxHopsChanged(hops) => {
                self.max_hops = hops;
                Command::none()
            }
            Message::TimeoutChanged(timeout) => {
                self.timeout_ms = timeout;
                Command::none()
            }
            Message::IpVersionChanged(version) => {
                self.ip_version = version;
                Command::none()
            }
            Message::CheckAsyncResults => self.check_results(),
        }
    }

    fn view(&'_ self) -> Element<'_, Message> {
        let input_row = row![
            text_input("🌏目标地址[IP|域名]", &self.target_input)
                .on_input(Message::TargetInputChanged)
                .width(Length::Fixed(300.0)),
            vertical_space().width(Length::Fixed(10.0)),
            checkbox("IPv4", self.ip_version == IpVersion::IPv4)
                .on_toggle(|_| Message::IpVersionChanged(IpVersion::IPv4)),
            checkbox("IPv6", self.ip_version == IpVersion::IPv6)
                .on_toggle(|_| Message::IpVersionChanged(IpVersion::IPv6)),
            vertical_space().width(Length::Fixed(10.0)),
            text("最大跳数:").width(Length::Fixed(80.0)),
            text_input("", &format!("{}", self.max_hops))
                .on_input(|input| {
                    Message::MaxHopsChanged(input.parse::<u32>().unwrap_or(30))
                })
                .width(Length::Fixed(40.0)),
            vertical_space().width(Length::Fixed(10.0)),
            text("超时(ms):").width(Length::Fixed(70.0)),
            text_input("", &format!("{}", self.timeout_ms))
                .on_input(|input| {
                    Message::TimeoutChanged(input.parse::<u64>().unwrap_or(1000))
                })
                .width(Length::Fixed(60.0)),
            vertical_space().width(Length::Fixed(10.0)),
            button(text(if self.is_tracing { "停止" } else { "开始" }))
                .on_press(if self.is_tracing {
                    Message::StopTrace
                } else {
                    Message::StartTrace
                }),
            vertical_space().width(Length::Fixed(10.0)),
            button(text("清理")).on_press(Message::ClearResults)
        ]
        .spacing(5)
        .height(50)
        .align_items(Alignment::Center);

        let table_header = row![
            table_header_label("跳数", 60.0),
            table_header_label("IP地址", 250.0),
            table_header_label("位置", 300.0),
            table_header_label("RTT1", 80.0),
            table_header_label("RTT2", 80.0),
            table_header_label("RTT3", 80.0),
            table_header_label("平均", 80.0)
        ]
        .spacing(0);

        let table_rows: Vec<Element<Message>> = self
            .current_hops
            .iter()
            .map(|hop| {
                let (flag, location_text) = if let Some(location) = self.geo_locations.get(&hop.ip) {
                    (get_country_flag(&location.lon), format_location_for_display(location))
                } else if hop.ip != "*" {
                    if let Some(location) = self.geo_service.get_cached_location(&hop.ip) {
                        (get_country_flag(&location.lon), format_location_for_display(&location))
                    } else {
                        ("".to_string(), "".to_string())
                    }
                } else {
                    ("".to_string(), "".to_string())
                };

                row![
                    table_data_col(format!("{}", hop.hop), 60.0, None),
                    table_data_col(&hop.ip, 250.0, None),
                    table_data_col(&flag, 50.0, Some(Font::with_name("Noto Color Emoji"))),
                    table_data_col(&location_text, 250.0, None),
                    table_data_col(rtt_to_string(hop.rtt1), 80.0, None),
                    table_data_col(rtt_to_string(hop.rtt2), 80.0, None),
                    table_data_col(rtt_to_string(hop.rtt3), 80.0, None),
                    table_data_col(rtt_to_string(hop.avg_rtt), 80.0, None)
                ]
                .into()
            })
            .collect();

        let table = column![
            table_header,
            scrollable(Column::with_children(table_rows).spacing(0)).height(Length::Fill)
        ]
        .spacing(5);

        let content = column![
            input_row,
            table,
            row![text(&self.status_message)].spacing(0)
        ].spacing(0).padding(0);

        container(content)
            .width(Length::Fill)
            .height(Length::Fill)
            .center_x()
            .into()
    }
}

fn rtt_to_string(rtt: Option<f64>) -> String {
    rtt.map_or("*".to_string(), |r| format!("{:.2}", r))
}

fn table_header_label<'a>(label: &str, width: f32) -> Container<'a, Message, Theme, Renderer> {
    container(text(label))
        .width(Length::Fixed(width))
        .height(Length::Fixed(30.0))
        .center_x()
        .align_y(Vertical::Center)
        .style(|_theme: &Theme| container::Appearance {
            border: iced::Border {
                color: iced::Color::from_rgba(0.0, 0.0, 0.0, 0.0),
                width: 1.0,
                radius: 0.2.into(),
            },
            shadow: iced::Shadow {
                offset: iced::Vector::new(0.0, 0.0),
                color: iced::Color::from_rgba(0.0, 0.0, 0.0, 0.3),
                blur_radius: 0.0,
            },
            background: Some(iced::Color::from_rgb(0.9, 0.9, 0.9).into()),
            ..Default::default()
        })
}

fn table_data_col<'a>(
    val: impl ToString,
    width: f32,
    font: Option<Font>,
) -> Container<'a, Message, Theme, Renderer> {
    container(match font {
        None => text(val),
        Some(font) => text(val).font(font),
    })
    .align_y(Vertical::Center)
    .width(Length::Fixed(width))
    .height(Length::Fixed(30.0))
    .center_x()
    .style(|_theme: &Theme| container::Appearance {
        border: iced::Border {
            color: iced::Color::from_rgba(0.0, 0.0, 0.0, 0.0),
            width: 1.0,
            radius: 0.2.into(),
        },
        background: Some(iced::Color::from_rgb(0.9, 0.9, 0.9).into()),
        ..Default::default()
    })
}
