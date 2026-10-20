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

use crate::access::AccessLog;
use crate::cache::{Caches, ObjectKey};

/// Shared state of the object routes.
#[derive(Clone, Debug)]
struct Objects {
    /// The caches.
    caches: Arc<Caches>,
    /// Where reads are recorded.
    access: Arc<AccessLog>,
}

impl Objects {
    /// Fetches an object and records the read, or returns the status to
    /// respond with.
    async fn fetch(
        &self,
        Path((cache, key)): &Path<(String, String)>,
        head: bool,
    ) -> Result<GetResult, StatusCode> {
        let found = self.caches.get(cache).ok_or(StatusCode::NOT_FOUND)?;
        let object =
            ObjectKey::try_from(key.as_str()).map_err(|_invalid| StatusCode::BAD_REQUEST)?;
        match found.get(&object, head).await {
            Ok(Some(hit)) => {
                self.access.record(hit.object);
                Ok(hit.result)
            }
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
}

/// Builds the router for object reads.
pub(super) fn router(caches: Arc<Caches>, access: Arc<AccessLog>) -> Router {
    Router::new()
        .route("/{cache}/{*key}", get(get_object).head(head_object))
        .with_state(Objects { caches, access })
}

/// Streams an object.
async fn get_object(state: State<Objects>, path: Path<(String, String)>) -> Response {
    match state.fetch(&path, false).await {
        Ok(result) => {
            let headers = object_headers(&result.meta);
            (headers, Body::from_stream(result.into_stream())).into_response()
        }
        Err(status) => status.into_response(),
    }
}

/// Returns the headers of an object without its content.
async fn head_object(state: State<Objects>, path: Path<(String, String)>) -> Response {
    match state.fetch(&path, true).await {
        Ok(result) => object_headers(&result.meta).into_response(),
        Err(status) => status.into_response(),
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
    use crate::cache::{Cache, CacheName, StoredObject};

    struct Response {
        status: StatusCode,
        headers: HeaderMap,
        body: String,
        reads: Vec<StoredObject>,
    }

    async fn store(objects: &[(&'static str, &'static str)]) -> Arc<dyn ObjectStore> {
        let store = InMemory::new();
        for (key, content) in objects {
            let key = ObjectPath::from(*key);
            store.put(&key, PutPayload::from(*content)).await.unwrap();
        }
        Arc::new(store)
    }

    async fn caches() -> Arc<Caches> {
        let main = Arc::new(Cache::new(
            "main-bucket".into(),
            ObjectPath::default(),
            store(&[("a/main", "main")]).await,
            None,
        ));
        let pr = Arc::new(Cache::new(
            "pr-bucket".into(),
            ObjectPath::from("prefix"),
            store(&[("a/pr", "pr")]).await,
            Some(Arc::clone(&main)),
        ));
        let mut caches = BTreeMap::new();
        caches.insert(CacheName::try_from("main".to_owned()).unwrap(), main);
        caches.insert(CacheName::try_from("pr".to_owned()).unwrap(), pr);
        Arc::new(Caches::new(caches))
    }

    async fn request(method: Method, uri: &str) -> Response {
        let access = Arc::new(AccessLog::default());
        let request = Request::builder()
            .method(method)
            .uri(uri)
            .body(Body::empty())
            .unwrap();
        let response = router(caches().await, Arc::clone(&access))
            .oneshot(request)
            .await
            .unwrap();
        let status = response.status();
        let headers = response.headers().clone();
        let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
        Response {
            status,
            headers,
            body: String::from_utf8(body.to_vec()).unwrap(),
            reads: access.take(),
        }
    }

    fn read(bucket: &str, key: &str) -> StoredObject {
        StoredObject {
            bucket: bucket.into(),
            key: ObjectPath::from(key),
        }
    }

    #[tokio::test]
    async fn get_returns_content() {
        let response = request(Method::GET, "/main/a/main").await;
        assert_eq!(response.status, StatusCode::OK, "the object exists");
        assert_eq!(response.headers[CONTENT_LENGTH], "4", "the length is set");
        assert!(
            response.headers.contains_key(LAST_MODIFIED),
            "the date is set"
        );
        assert_eq!(response.body, "main", "the content is returned");
        assert_eq!(
            response.reads,
            [read("main-bucket", "a/main")],
            "the read is recorded"
        );
    }

    #[tokio::test]
    async fn head_returns_headers_only() {
        let response = request(Method::HEAD, "/main/a/main").await;
        assert_eq!(response.status, StatusCode::OK, "the object exists");
        assert_eq!(response.headers[CONTENT_LENGTH], "4", "the length is set");
        assert!(response.body.is_empty(), "no content is returned");
        assert_eq!(
            response.reads,
            [read("main-bucket", "a/main")],
            "the read is recorded"
        );
    }

    #[tokio::test]
    async fn reads_include_the_prefix() {
        let response = request(Method::GET, "/pr/a/pr").await;
        assert_eq!(
            response.reads,
            [read("pr-bucket", "prefix/a/pr")],
            "the key includes the prefix"
        );
    }

    #[tokio::test]
    async fn get_uses_fallback() {
        let response = request(Method::GET, "/pr/a/main").await;
        assert_eq!(
            response.status,
            StatusCode::OK,
            "the fallback has the object"
        );
        assert_eq!(response.body, "main", "the content comes from the fallback");
        assert_eq!(
            response.reads,
            [read("main-bucket", "a/main")],
            "the read is recorded for the fallback"
        );
    }

    #[tokio::test]
    async fn fallback_is_one_way() {
        let response = request(Method::GET, "/main/a/pr").await;
        assert_eq!(
            response.status,
            StatusCode::NOT_FOUND,
            "main never reads pr"
        );
    }

    #[tokio::test]
    async fn misses() {
        for uri in ["/main/a/missing", "/unknown/a/main"] {
            let response = request(Method::GET, uri).await;
            assert_eq!(response.status, StatusCode::NOT_FOUND, "{uri} is not found");
            assert!(response.reads.is_empty(), "a miss is not recorded");
        }
        let response = request(Method::GET, "/main/a/../main").await;
        assert_eq!(
            response.status,
            StatusCode::BAD_REQUEST,
            "dot segments are rejected"
        );
    }
}
