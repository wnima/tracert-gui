#![windows_subsystem = "windows"]
use iced::window::Icon;
use iced::window::icon::from_rgba;
use iced::{Application, Font, Pixels, Settings, window};
use image::GenericImageView;
use log::{error, info};

mod firewall;
mod geoip;
mod gui;
mod icmp;
mod traceroute;

use firewall::FirewallManager;
use gui::TracerouteApp;

fn run_gui_mode() -> iced::Result {
    unsafe {
        std::env::set_var("WGPU_BACKEND", "gl");
    }
    
    let settings = Settings {
        window: window::Settings {
            size: iced::Size::new(950.0, 500.0),
            max_size: Some(iced::Size::new(950.0, 500.0)),
            min_size: Some(iced::Size::new(950.0, 500.0)),
            icon: load_window_icon(),
            resizable: false,
            ..Default::default()
        },
        default_font: Font::with_name("Microsoft YaHei"),
        default_text_size: Pixels(12.0),
        ..Default::default()
    };
    TracerouteApp::run(settings)
}

fn load_window_icon() -> Option<Icon> {
    let icon_data: &'static [u8] = include_bytes!("../assets/icon.ico");
    if let Ok(image) = image::load_from_memory(icon_data) {
        let rgba = image.to_rgba8();
        let (width, height) = image.dimensions();
        from_rgba(rgba.to_vec(), width, height).ok()
    } else {
        error!("无法加载图标");
        None
    }
}

fn main() -> iced::Result {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();
    
    // 检查并配置防火墙规则
    if !FirewallManager::check_icmp_firewall_rule() {
        info!("防火墙规则未找到，正在添加...");
        match FirewallManager::add_icmp_firewall_rule() {
            Ok(()) => {
                if FirewallManager::check_icmp_firewall_rule() {
                    info!("防火墙规则添加成功");
                } else {
                    error!("防火墙规则可能未正确添加");
                }
            }
            Err(e) => {
                error!("添加防火墙规则失败: {}", e);
            }
        }
    } else {
        info!("防火墙规则已存在");
    }

    run_gui_mode()
}
