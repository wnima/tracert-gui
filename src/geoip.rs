use anyhow::{Result, anyhow};
use ipdb::Reader;
use ipdb::city::CityInfo;
use log::error;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

static IPDB_BYTES: &[u8] = include_bytes!("../assets/qqwry.ipdb");

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GeoLocation {
    pub ip: String,
    pub country: String,
    pub country_code: String,
    pub region: String,
    pub region_name: String,
    pub city: String,
    pub zip: String,
    pub lat: String,
    pub lon: String,
    pub timezone: String,
    pub isp: String,
    pub org: String,
    pub as_number: String,
    pub query: String,
}

impl Default for GeoLocation {
    fn default() -> Self {
        Self {
            ip: String::new(),
            country: String::new(),
            country_code: String::new(),
            region: String::new(),
            region_name: String::new(),
            city: String::new(),
            zip: String::new(),
            lat: String::new(),
            lon: String::new(),
            timezone: String::new(),
            isp: String::new(),
            org: String::new(),
            as_number: String::new(),
            query: String::new(),
        }
    }
}

pub struct GeoIPService {
    reader: Option<Reader<Vec<u8>>>,
    cache: HashMap<String, GeoLocation>,
}

impl Clone for GeoIPService {
    fn clone(&self) -> Self {
        Self {
            reader: Self::load_local_db(),
            cache: self.cache.clone(),
        }
    }
}

impl GeoIPService {
    pub fn new() -> Self {
        Self {
            reader: Self::load_local_db(),
            cache: HashMap::new(),
        }
    }

    fn load_local_db() -> Option<Reader<Vec<u8>>> {
        match Reader::from_source(IPDB_BYTES.to_vec()) {
            Ok(db) => Some(db),
            Err(e) => {
                error!("加载IP数据库失败: {}", e);
                None
            }
        }
    }

    pub async fn get_location(&mut self, ip: &str) -> Result<GeoLocation> {
        if let Some(cached) = self.cache.get(ip) {
            return Ok(cached.clone());
        }

        let location = match &self.reader {
            Some(reader) => {
                match reader.lookup_prefix(ip.parse()?, "CN".to_owned()) {
                    Ok((city_info, _)) => Ok(Self::convert_to_geolocation(ip, city_info)),
                    Err(err) => Err(anyhow!("查询IP失败: {}", err)),
                }
            }
            None => Err(anyhow!("未找到本地IP数据库")),
        }?;

        self.cache.insert(ip.to_string(), location.clone());
        Ok(location)
    }

    fn convert_to_geolocation(ip: &str, city_info: CityInfo) -> GeoLocation {
        let country_code = if !city_info.country_code.is_empty() {
            city_info.country_code.to_string()
        } else {
            "XX".to_string()
        };

        GeoLocation {
            ip: ip.to_string(),
            country: city_info.country_name.to_string(),
            country_code,
            region: city_info.region_name.to_string(),
            region_name: city_info.region_name.to_string(),
            city: city_info.city_name.to_string(),
            zip: String::new(),
            lat: city_info.latitude.to_string(),
            lon: city_info.longitude.to_string(),
            timezone: city_info.timezone.to_string(),
            isp: city_info.isp_domain.to_string(),
            org: city_info.owner_domain.to_string(),
            as_number: String::new(),
            query: ip.to_string(),
        }
    }

    pub fn get_cached_location(&self, ip: &str) -> Option<GeoLocation> {
        self.cache.get(ip).cloned()
    }
}

/// 格式化地理位置信息
pub fn format_location_for_display(location: &GeoLocation) -> String {
    let mut parts = Vec::new();

    if !location.city.is_empty() {
        parts.push(location.city.clone());
    }
    if !location.region_name.is_empty() {
        parts.push(location.region_name.clone());
    }
    if !location.country.is_empty() 
        && location.country != "Unknown" 
        && location.country != "Local Network" 
    {
        parts.push(location.country.clone());
    }
    if !location.isp.is_empty() {
        parts.push(location.isp.clone());
    }

    if parts.is_empty() {
        "位置未知".to_string()
    } else {
        parts.join(", ")
    }
}

/// 获取国家代码对应的旗帜表情符号
pub fn get_country_flag(country_code: &str) -> String {
    match country_code.to_uppercase().as_str() {
        "CN" => "🇨🇳",
        "US" => "🇺🇸",
        "JP" => "🇯🇵",
        "KR" => "🇰🇷",
        "UK" | "GB" => "🇬🇧",
        "DE" => "🇩🇪",
        "FR" => "🇫🇷",
        "CA" => "🇨🇦",
        "AU" => "🇦🇺",
        "IN" => "🇮🇳",
        "BR" => "🇧🇷",
        "RU" => "🇷🇺",
        "IT" => "🇮🇹",
        "ES" => "🇪🇸",
        "NL" => "🇳🇱",
        "SE" => "🇸🇪",
        "NO" => "🇳🇴",
        "FI" => "🇫🇮",
        "DK" => "🇩🇰",
        "CH" => "🇨🇭",
        "AT" => "🇦🇹",
        "BE" => "🇧🇪",
        "PL" => "🇵🇱",
        "CZ" => "🇨🇿",
        "SK" => "🇸🇰",
        "HU" => "🇭🇺",
        "GR" => "🇬🇷",
        "PT" => "🇵🇹",
        "IE" => "🇮🇪",
        "NZ" => "🇳🇿",
        "SG" => "🇸🇬",
        "MY" => "🇲🇾",
        "TH" => "🇹🇭",
        "VN" => "🇻🇳",
        "PH" => "🇵🇭",
        "ID" => "🇮🇩",
        "MX" => "🇲🇽",
        "AR" => "🇦🇷",
        "CL" => "🇨🇱",
        "CO" => "🇨🇴",
        "PE" => "🇵🇪",
        "ZA" => "🇿🇦",
        "EG" => "🇪🇬",
        "IL" => "🇮🇱",
        "TR" => "🇹🇷",
        "SA" => "🇸🇦",
        "AE" => "🇦🇪",
        _ => "🌐",
    }.to_string()
}
