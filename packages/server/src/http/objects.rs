//! Reading objects from caches.

use core::error::Error;
use std::sync::Arc;

use axum::Router;
use axum::body::Body;
use axum::extract::{Path, State};
use axum::http::header::{CONTENT_LENGTH, CONTENT_TYPE, ETAG, LAST_MODIFIED};
use axum::http::{HeaderMap, HeaderValue, StatusCode};
use axum::response::{IntoResponse as _, Response};
use axum::routing::get;
use object_store::{GetResult, ObjectMeta};

use crate::cache::{Caches, ObjectKey};

/// Builds the router for object reads.
pub(super) fn router(caches: Arc<Caches>) -> Router {
    Router::new()
        .route("/{cache}/{*key}", get(get_object).head(head_object))
        .with_state(caches)
}

/// Streams an object.
async fn get_object(state: State<Arc<Caches>>, path: Path<(String, String)>) -> Response {
    match fetch(&state, &path, false).await {
        Ok(result) => {
            let headers = object_headers(&result.meta);
            (headers, Body::from_stream(result.into_stream())).into_response()
        }
        Err(status) => status.into_response(),
    }
}

/// Returns the headers of an object without its content.
async fn head_object(state: State<Arc<Caches>>, path: Path<(String, String)>) -> Response {
    match fetch(&state, &path, true).await {
        Ok(result) => object_headers(&result.meta).into_response(),
        Err(status) => status.into_response(),
    }
}

/// Fetches an object, or the status to respond with if that fails.
async fn fetch(
    caches: &Caches,
    Path((cache, key)): &Path<(String, String)>,
    head: bool,
) -> Result<GetResult, StatusCode> {
    let found = caches.get(cache).ok_or(StatusCode::NOT_FOUND)?;
    let object = ObjectKey::try_from(key.as_str()).map_err(|_invalid| StatusCode::BAD_REQUEST)?;
    match found.get(&object, head).await {
        Ok(Some(result)) => Ok(result),
        Ok(None) => Err(StatusCode::NOT_FOUND),
        Err(err) => {
            tracing::error!(
                error = &err as &dyn Error,
                cache,
                key,
                "failed to fetch an object"
            );
            Err(StatusCode::BAD_GATEWAY)
        }
    }
}

/// The response headers that describe an object.
fn object_headers(meta: &ObjectMeta) -> HeaderMap {
    let mut headers = HeaderMap::new();
    headers.insert(
        CONTENT_TYPE,
        HeaderValue::from_static("application/octet-stream"),
    );
    headers.insert(CONTENT_LENGTH, HeaderValue::from(meta.size));
    let last_modified = meta
        .last_modified
        .format("%a, %d %b %Y %H:%M:%S GMT")
        .to_string();
    if let Ok(value) = HeaderValue::from_str(&last_modified) {
        headers.insert(LAST_MODIFIED, value);
    }
    if let Some(Ok(value)) = meta.e_tag.as_deref().map(HeaderValue::from_str) {
        headers.insert(ETAG, value);
    }
    headers
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use axum::body::to_bytes;
    use axum::http::{Method, Request};
    use object_store::memory::InMemory;
    use object_store::path::Path as ObjectPath;
    use object_store::{ObjectStore, ObjectStoreExt as _, PutPayload};
    use tower::ServiceExt as _;

    use super::*;
    use crate::cache::{Cache, CacheName};

    async fn store(objects: &[(&'static str, &'static str)]) -> Arc<dyn ObjectStore> {
        let store = InMemory::new();
        for (key, content) in objects {
            let key = ObjectPath::from(*key);
            store.put(&key, PutPayload::from(*content)).await.unwrap();
        }
        Arc::new(store)
    }

    async fn caches() -> Arc<Caches> {
        let main = Arc::new(Cache::new(store(&[("a/main", "main")]).await, None));
        let pr = Arc::new(Cache::new(
            store(&[("a/pr", "pr")]).await,
            Some(Arc::clone(&main)),
        ));
        let mut caches = BTreeMap::new();
        caches.insert(CacheName::try_from("main".to_owned()).unwrap(), main);
        caches.insert(CacheName::try_from("pr".to_owned()).unwrap(), pr);
        Arc::new(Caches::new(caches))
    }

    async fn request(method: Method, uri: &str) -> (StatusCode, HeaderMap, String) {
        let request = Request::builder()
            .method(method)
            .uri(uri)
            .body(Body::empty())
            .unwrap();
        let response = router(caches().await).oneshot(request).await.unwrap();
        let status = response.status();
        let headers = response.headers().clone();
        let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
        (status, headers, String::from_utf8(body.to_vec()).unwrap())
    }

    #[tokio::test]
    async fn get_returns_content() {
        let (status, headers, body) = request(Method::GET, "/main/a/main").await;
        assert_eq!(status, StatusCode::OK, "the object exists");
        assert_eq!(headers[CONTENT_LENGTH], "4", "the length is set");
        assert!(headers.contains_key(LAST_MODIFIED), "the date is set");
        assert_eq!(body, "main", "the content is returned");
    }

    #[tokio::test]
    async fn head_returns_headers_only() {
        let (status, headers, body) = request(Method::HEAD, "/main/a/main").await;
        assert_eq!(status, StatusCode::OK, "the object exists");
        assert_eq!(headers[CONTENT_LENGTH], "4", "the length is set");
        assert!(body.is_empty(), "no content is returned");
    }

    #[tokio::test]
    async fn get_uses_fallback() {
        let (status, _, body) = request(Method::GET, "/pr/a/main").await;
        assert_eq!(status, StatusCode::OK, "the fallback has the object");
        assert_eq!(body, "main", "the content comes from the fallback");
    }

    #[tokio::test]
    async fn fallback_is_one_way() {
        let (status, ..) = request(Method::GET, "/main/a/pr").await;
        assert_eq!(status, StatusCode::NOT_FOUND, "main never reads pr");
    }

    #[tokio::test]
    async fn misses() {
        for uri in ["/main/a/missing", "/unknown/a/main"] {
            let (status, ..) = request(Method::GET, uri).await;
            assert_eq!(status, StatusCode::NOT_FOUND, "{uri} is not found");
        }
        let (status, ..) = request(Method::GET, "/main/a/../main").await;
        assert_eq!(status, StatusCode::BAD_REQUEST, "dot segments are rejected");
    }
}
