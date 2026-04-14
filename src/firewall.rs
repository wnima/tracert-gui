use log::info;
use std::ptr;
use std::thread;
use std::time::Duration;

pub struct FirewallManager;

const OUTBOUND_RULE_NAME: &str = "TracertToolsOutboundICMP";
const INBOUND_RULE_NAME: &str = "TracertToolsInboundICMP";
const IPV6_OUTBOUND_RULE_NAME: &str = "TracertToolsOutboundICMPv6";
const IPV6_INBOUND_RULE_NAME: &str = "TracertToolsInboundICMPv6";

impl FirewallManager {
    /// 检查防火墙规则是否存在
    pub fn check_icmp_firewall_rule() -> bool {
        use windows::Win32::NetworkManagement::WindowsFirewall::{INetFwPolicy2, NetFwPolicy2};
        use windows::Win32::System::Com::{
            CLSCTX_ALL, COINIT_MULTITHREADED, CoCreateInstance, CoInitializeEx, CoUninitialize,
        };
        use windows::core::BSTR;

        unsafe {
            let _ = CoInitializeEx(Some(ptr::null()), COINIT_MULTITHREADED);

            let policy: INetFwPolicy2 = match CoCreateInstance(&NetFwPolicy2, None, CLSCTX_ALL) {
                Ok(p) => p,
                Err(e) => {
                    info!("创建INetFwPolicy2失败: {:?}", e);
                    CoUninitialize();
                    return false;
                }
            };

            let rules = match policy.Rules() {
                Ok(r) => r,
                Err(e) => {
                    info!("获取Rules失败: {:?}", e);
                    CoUninitialize();
                    return false;
                }
            };

            let names = [
                OUTBOUND_RULE_NAME,
                INBOUND_RULE_NAME,
                IPV6_OUTBOUND_RULE_NAME,
                IPV6_INBOUND_RULE_NAME,
            ];

            for &name in names.iter() {
                let b = BSTR::from(name);
                if rules.Item(&b).is_ok() {
                    drop(rules);
                    drop(policy);
                    CoUninitialize();
                    return true;
                }
            }

            drop(rules);
            drop(policy);
            CoUninitialize();
            false
        }
    }

    /// 使用COM接口添加防火墙规则
    fn add_icmp_firewall_rule_com(
        rule_name: &str,
        outbound: bool,
        inbound: bool,
        is_ipv6: bool,
    ) -> Result<(), String> {
        use windows::Win32::NetworkManagement::WindowsFirewall::{
            INetFwPolicy2, INetFwRule, INetFwRules, NET_FW_ACTION_ALLOW, NET_FW_PROFILE2_ALL,
            NET_FW_RULE_DIR_IN, NET_FW_RULE_DIR_OUT, NetFwPolicy2, NetFwRule,
        };
        use windows::Win32::System::Com::{
            CLSCTX_ALL, COINIT_MULTITHREADED, CoCreateInstance, CoInitializeEx, CoUninitialize,
        };
        use windows::core::BSTR;
        use std::ptr;

        unsafe {
            let hr = CoInitializeEx(Some(ptr::null()), COINIT_MULTITHREADED);
            if hr.is_err() {
                return Err(format!("COM初始化失败: {:?}", hr));
            }

            let policy: INetFwPolicy2 = match CoCreateInstance(&NetFwPolicy2, None, CLSCTX_ALL) {
                Ok(p) => p,
                Err(e) => {
                    CoUninitialize();
                    return Err(format!("无法创建INetFwPolicy2: {:?}", e));
                }
            };

            let rules: INetFwRules = match policy.Rules() {
                Ok(r) => r,
                Err(e) => {
                    drop(policy);
                    CoUninitialize();
                    return Err(format!("无法获取Rules: {:?}", e));
                }
            };

            let protocol = if is_ipv6 { 58 } else { 1 }; // ICMPv6 or ICMP

            // 添加出站规则
            if outbound {
                let out_rule: INetFwRule = match CoCreateInstance(&NetFwRule, None, CLSCTX_ALL) {
                    Ok(r) => r,
                    Err(_) => {
                        drop(rules);
                        drop(policy);
                        CoUninitialize();
                        return Err("无法创建出站规则".to_string());
                    }
                };
                let _ = out_rule.SetName(&BSTR::from(rule_name));
                let _ = out_rule.SetEnabled(windows::Win32::Foundation::VARIANT_TRUE);
                let _ = out_rule.SetProfiles(NET_FW_PROFILE2_ALL.0);
                let _ = out_rule.SetProtocol(protocol);
                let _ = out_rule.SetDirection(NET_FW_RULE_DIR_OUT);
                let _ = out_rule.SetAction(NET_FW_ACTION_ALLOW);
                let _ = rules.Add(&out_rule);
                drop(out_rule);
            }

            // 添加入站规则
            if inbound {
                let in_rule: INetFwRule = match CoCreateInstance(&NetFwRule, None, CLSCTX_ALL) {
                    Ok(r) => r,
                    Err(_) => {
                        drop(rules);
                        drop(policy);
                        CoUninitialize();
                        return Err("无法创建入站规则".to_string());
                    }
                };
                let _ = in_rule.SetName(&BSTR::from(rule_name));
                let _ = in_rule.SetEnabled(windows::Win32::Foundation::VARIANT_TRUE);
                let _ = in_rule.SetProfiles(NET_FW_PROFILE2_ALL.0);
                let _ = in_rule.SetProtocol(protocol);
                let _ = in_rule.SetDirection(NET_FW_RULE_DIR_IN);
                let _ = in_rule.SetAction(NET_FW_ACTION_ALLOW);
                let _ = rules.Add(&in_rule);
                drop(in_rule);
            }

            drop(rules);
            drop(policy);
            CoUninitialize();
            info!("防火墙规则添加成功: {}", rule_name);
            Ok(())
        }
    }

    /// 添加ICMP防火墙规则
    pub fn add_icmp_firewall_rule() -> Result<(), String> {
        info!("正在添加ICMP防火墙规则...");
        
        Self::add_icmp_firewall_rule_com(OUTBOUND_RULE_NAME, true, false, false)?;
        Self::add_icmp_firewall_rule_com(INBOUND_RULE_NAME, false, true, false)?;
        Self::add_icmp_firewall_rule_com(IPV6_OUTBOUND_RULE_NAME, true, false, true)?;
        Self::add_icmp_firewall_rule_com(IPV6_INBOUND_RULE_NAME, false, true, true)?;
        
        thread::sleep(Duration::from_millis(1000));
        Ok(())
    }

}
