use std::{path::PathBuf, str::FromStr};

use axum::{
    extract::Path,
    http::StatusCode,
    response::{IntoResponse, Response},
};

pub fn add(left: u64, right: u64) -> u64 {
    left + right
}

fn content_type(extension: &str) -> &'static str {
    match extension {
        "ico" => "image/vnd.microsoft.icon",
        "css" => "text/css",
        "html" => "text/html",
        "ttf" => "font/truetype",
        "otf" => "font/opentype",
        "svg" => "image/svg+xml",
        _ => panic!("{extension} unknown"),
    }
}

pub async fn server_asset(Path(filename): Path<String>) -> Response {
    println!("retrieve server asset for {filename}");
    // TODO: This should handle HTTP 404's separately from
    // all of the other 404s (e.g. missing assets)
    match website::asset(&filename) {
        Some(asset) => {
            let file_type = {
                let path = PathBuf::from_str(&filename).unwrap();
                let extension = path.extension().unwrap();
                String::from_utf8(extension.as_encoded_bytes().to_vec()).unwrap()
            };

            let content_type = content_type(&file_type);
            let response = ([(axum::http::header::CONTENT_TYPE, content_type)], asset);
            response.into_response()
        }
        None => StatusCode::NOT_FOUND.into_response(),
    }
}


#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn it_works() {
        let result = add(2, 2);
        assert_eq!(result, 4);
    }
}
