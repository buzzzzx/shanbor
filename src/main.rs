use anyhow::Result;
use axum::{
    extract::{Path, Extension}, 
    handler::get, 
    http::{StatusCode, HeaderMap, HeaderValue}, 
    Router,
    AddExtensionLayer,
};
use bytes::Bytes;
use lru::LruCache;
use percent_encoding::{percent_decode_str, percent_encode, NON_ALPHANUMERIC};
use serde::Deserialize;
use std::{collections::hash_map::DefaultHasher, convert::TryInto, hash::{Hash, Hasher}, sync::Arc};
use tokio::sync::Mutex;
use tower::ServiceBuilder;
use tracing::{info, instrument};

// 声明 pb, engine 模块，Rust 根据名字去加载该模块内容
mod pb;
mod engine;

use pb::*;
use engine::{Engine, Photon};
use image::ImageOutputFormat;

// 参数使用 serde 做 Deserialize，axum 会自动识别并解析
 #[derive(Deserialize)]
 struct Params {
     spec: String,
     url: String,
 }

 type Cache = Arc<Mutex<LruCache<u64, Bytes>>>;

#[tokio::main]
async fn main() {
    // 初始化 tracing 日志追踪
    tracing_subscriber::fmt::init();
    let cache: Cache = Arc::new(Mutex::new(LruCache::new(1024)));

    // 构建路由
    let app = Router::new()
        // "GET /image" 会执行 generate 函数，并把 spec 和 url 传递过去
        .route("/image/:spec/:url", get(generate))
        .layer(
            ServiceBuilder::new()
                .layer(AddExtensionLayer::new(cache))
                .into_inner(),
        );
    
    // 运行 web 服务器
    let addr = "127.0.0.1:3000".parse().unwrap();
    
    // 辅助调试
    print_test_url("https://p8.pstatp.com/origin/pgc-image/e80c318c4b84494abd47302647f4b6e3.jpeg");
    // print_test_url("https://images.pexels.com/photos/1562477/pexels-photo-1562477.jpeg?auto=compress&cs=tinysrgb&dpr=3&h=750&w=1260");

    info!("Listening on {}", addr);

    axum::Server::bind(&addr)
        .serve(app.into_make_service())
        .await
        .unwrap();
}

async fn generate(
    Path(Params {spec, url}): Path<Params>,
    Extension(cache): Extension<Cache>
) -> Result<(HeaderMap, Vec<u8>), StatusCode> {
    // 图片转换指令 ImageSpec
    let spec: ImageSpec = spec
        .as_str()
        .try_into()
        .map_err(|_| StatusCode::BAD_REQUEST)?;
    // 图片 URL
    let url: &str = &percent_decode_str(&url).decode_utf8_lossy();
    // 图片数据 Bytes
    let data = retrieve_image(&url, cache)
        .await
        .map_err(|_| StatusCode::BAD_REQUEST)?;

    // 根据图片指令处理图片
    // 使用 image engine 处理
    let mut engine: Photon = data
        .try_into()
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    engine.apply(&spec.specs);

    let image = engine.generate(ImageOutputFormat::Jpeg(85));

    info!("Finished processing: image size {}", image.len());

    let mut headers = HeaderMap::new();
    headers.insert("content-type", HeaderValue::from_static("image/jpeg"));

    Ok((headers, image))
}

#[instrument(level = "info", skip(cache))]
async fn retrieve_image(url: &str, cache: Cache) -> Result<Bytes> {
    let mut hasher = DefaultHasher::new();
    url.hash(&mut hasher);
    let key = hasher.finish();

    let g = &mut cache.lock().await;
    let data = match g.get(&key) {
        Some(v) => {
            info!("Mache cache {}", key);
            v.to_owned()
        },
        None => {
            info!("Retrieve url");
            let resp = reqwest::get(url).await?;
            let data = resp.bytes().await?;
            g.put(key, data.clone());
            data
        }
    };

    Ok(data)
}

// 调试辅助函数
fn print_test_url(url: &str) {
    use std::borrow::Borrow;
    let spec1 = Spec::new_resize(500, 800, resize::SampleFilter::CatmullRom);
    let spec2 = Spec::new_watermark(20, 20);
    let spec3 = Spec::new_filter(filter::Filter::Marine);
    let image_spec = ImageSpec::new(vec![spec1, spec2, spec3]);
    let s: String = image_spec.borrow().into();
    let test_image = percent_encode(url.as_bytes(), NON_ALPHANUMERIC).to_string();
    println!("test url: http://localhost:3000/image/{}/{}", s, test_image);
}