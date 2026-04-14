use std::env;
use std::fs;
use std::fs::File;
use std::io::{BufWriter, Read, Write};
use std::path::Path;

fn main() {
    if env::var_os("CARGO_CFG_WINDOWS").is_some() {
        let _ = embed_resource::compile("assets/app.rc", embed_resource::NONE);
        println!("cargo:rerun-if-changed=assets/app.rc");
        println!("cargo:rerun-if-changed=assets/app.manifest");
        println!("cargo:rerun-if-changed=assets/icon.ico");
    }

    // 检查并准备 IP 数据库文件
    let project_root = env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR not set");
    let assets_dir = Path::new(&project_root).join("assets");
    let ipdb_path = assets_dir.join("qqwry.ipdb");

    // 确保 assets 目录存在
    fs::create_dir_all(&assets_dir).expect("Failed to create assets directory");

    // 检查 IPDB 文件是否存在
    if !ipdb_path.exists() {
        download_ipdb_file(&ipdb_path.as_path().to_str().unwrap()).expect("IP数据库下载失败");
    } else {
        println!("Using existing qqwry.ipdb file from assets/ directory.");
    }

    // 告诉 cargo 在文件更改时重新运行构建脚本
    println!("rerun-if-changed={}", ipdb_path.display());
}

fn download_ipdb_file(path: &str) -> Result<(), Box<dyn std::error::Error>> {
    // IP 地理位置数据库可能的下载源
    let url = "https://github.com/nmgliangwei/qqwry.ipdb/releases/download/2026-01-07/qqwry.ipdb";
    println!("Attempting to download from: {}", url);

    match download_large_file(url, path) {
        Ok(_) => {
            println!("Successfully downloaded IP database from: {}", url);
            return Ok(());
        }
        Err(e) => {
            println!("Download failed from {}: {}", url, e);
        }
    }

    Err("All download attempts failed".into())
}

fn download_large_file(url: &str, dest_path: &str) -> Result<(), Box<dyn std::error::Error>> {
    // 1. 创建一个 ureq agent（客户端实例），可以配置超时等
    let agent = ureq::agent();

    // 2. 发起 GET 请求
    let response = agent.get(url).call()?;

    // 3. 检查响应状态码
    if !response.status().is_success() {
        return Err(format!("Request failed with status: {}", response.status()).into());
    }

    // 4. 获取响应体的读取器
    let mut body = response.into_body();

    let mut reader = body.as_reader();

    // 5. 打开目标文件，准备写入
    let file = File::create(dest_path)?;
    // 使用 BufWriter 可以提高写入效率
    let mut writer = BufWriter::new(file);

    // 6. 流式复制数据
    let mut downloaded_bytes = 0;
    loop {
        let mut buffer = [0u8; 8192]; // 定义一个 8KB 的缓冲区
        let bytes_read = reader.read(&mut buffer)?;
        if bytes_read == 0 {
            break; // 没有更多数据可读，下载完成
        }
        writer.write_all(&buffer[..bytes_read])?;
        downloaded_bytes += bytes_read;
    }

    // 7. 确保所有数据都被刷写到磁盘
    writer.flush()?;

    println!("Downloaded {} bytes to {}", downloaded_bytes, dest_path);
    Ok(())
}
