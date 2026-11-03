use crate::openapi;
use reqwest::Method;
use reqwest::RequestBuilder;
use reqwest::Response;
use reqwest::StatusCode;
use reqwest::header::AUTHORIZATION;
use reqwest::header::CONTENT_TYPE;
use reqwest::header::COOKIE;
use reqwest::header::HeaderMap;
use reqwest::header::HeaderValue;
use reqwest::header::LOCATION;
use reqwest::header::SET_COOKIE;
use reqwest::redirect::Policy;
use serde_json::Value;
use std::collections::BTreeMap;
use std::net::SocketAddr;
use tokio::net::TcpListener;
use tokio_util::sync::CancellationToken;
use topcoat::router::Router;

pub struct TestServer {
    address: SocketAddr,
    shutdown: CancellationToken,
}

impl TestServer {
    pub fn new(address: SocketAddr, shutdown: CancellationToken) -> Self {
        Self { address, shutdown }
    }

    pub async fn serve(router: Router) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let shutdown = CancellationToken::new();
        tokio::spawn(topcoat::serve_until(
            listener,
            router,
            shutdown.clone().cancelled_owned(),
        ));
        Self::new(address, shutdown)
    }

    pub fn browser(&self) -> Browser {
        Browser {
            client: reqwest::Client::builder()
                .redirect(Policy::none())
                .build()
                .unwrap(),
            base: self.url(""),
            cookies: BTreeMap::new(),
            headers: HeaderMap::new(),
        }
    }

    pub async fn request(
        &self,
        method: Method,
        path: &str,
        token: Option<&str>,
        body: Option<Value>,
    ) -> (StatusCode, Value) {
        self.request_with_headers(method, path, token, HeaderMap::new(), body)
            .await
    }

    pub async fn request_with_headers(
        &self,
        method: Method,
        path: &str,
        token: Option<&str>,
        mut headers: HeaderMap,
        body: Option<Value>,
    ) -> (StatusCode, Value) {
        if let Some(token) = token {
            headers.insert(
                AUTHORIZATION,
                HeaderValue::from_str(&format!("Bearer {token}")).unwrap(),
            );
        }
        let (status, _, body) = self
            .request_with_response_headers(method, path, headers, body)
            .await;
        (status, body)
    }

    pub async fn request_with_response_headers(
        &self,
        method: Method,
        path: &str,
        headers: HeaderMap,
        body: Option<Value>,
    ) -> (StatusCode, HeaderMap, Value) {
        let mut request = reqwest::Client::new()
            .request(method.clone(), self.url(path))
            .headers(headers);
        if let Some(body) = body {
            request = request.json(&body);
        }
        let response = request.send().await.unwrap();
        let status = response.status();
        let headers = response.headers().clone();
        let content_type = media_type(&response);
        let bytes = response.bytes().await.unwrap();
        let body = openapi::check_response(
            method.as_str(),
            path.split('?').next().unwrap(),
            status.as_u16(),
            content_type.as_deref(),
            &bytes,
        );
        (status, headers, body)
    }

    pub async fn get(&self, path: &str) -> (StatusCode, Value) {
        self.request(Method::GET, path, None, None).await
    }

    pub async fn get_bytes(&self, path: &str) -> (StatusCode, HeaderMap, Vec<u8>) {
        let response = reqwest::get(self.url(path)).await.unwrap();
        let status = response.status();
        let headers = response.headers().clone();
        let content_type = media_type(&response);
        let body = response.bytes().await.unwrap().to_vec();
        openapi::check_response(
            Method::GET.as_str(),
            path.split('?').next().unwrap(),
            status.as_u16(),
            content_type.as_deref(),
            &body,
        );
        (status, headers, body)
    }

    pub async fn get_text(&self, path: &str) -> (StatusCode, HeaderMap, String) {
        let response = reqwest::get(self.url(path)).await.unwrap();
        (
            response.status(),
            response.headers().clone(),
            response.text().await.unwrap(),
        )
    }

    pub async fn post_protobuf(
        &self,
        path: &str,
        token: &str,
        body: Vec<u8>,
    ) -> (StatusCode, Vec<u8>) {
        let response = reqwest::Client::new()
            .post(self.url(path))
            .header(AUTHORIZATION, format!("Bearer {token}"))
            .header(CONTENT_TYPE, openapi::PROTOBUF)
            .body(body)
            .send()
            .await
            .unwrap();
        let status = response.status();
        let content_type = media_type(&response);
        openapi::check_protobuf(
            Method::POST.as_str(),
            path,
            status.as_u16(),
            content_type.as_deref(),
        );
        (status, response.bytes().await.unwrap().to_vec())
    }

    pub fn shut_down(&self) {
        self.shutdown.cancel();
    }

    fn url(&self, path: &str) -> String {
        format!("http://{}{path}", self.address)
    }
}

impl Drop for TestServer {
    fn drop(&mut self) {
        self.shutdown.cancel();
    }
}

fn media_type(response: &Response) -> Option<String> {
    response.headers().get(CONTENT_TYPE).map(|x| {
        x.to_str()
            .unwrap()
            .split(';')
            .next()
            .unwrap()
            .trim()
            .to_owned()
    })
}

pub struct Browser {
    client: reqwest::Client,
    base: String,
    cookies: BTreeMap<String, String>,
    headers: HeaderMap,
}

pub struct Page {
    pub status: StatusCode,
    pub location: Option<String>,
    pub set_cookies: Vec<String>,
    pub body: String,
}

impl Page {
    pub fn csrf_token(&self) -> String {
        let field = r#"name="csrf" value=""#;
        let start = self
            .body
            .find(field)
            .unwrap_or_else(|| panic!("the page has no CSRF field\n{}", self.body))
            + field.len();
        let length = self.body[start..].find('"').unwrap();
        self.body[start..start + length].to_owned()
    }
}

impl Browser {
    pub async fn get(&mut self, path: &str) -> Page {
        let request = self.client.get(format!("{}{path}", self.base));
        self.send(Method::GET, path, request).await
    }

    pub async fn post_form(&mut self, path: &str, fields: &[(&str, &str)]) -> Page {
        let body = url::form_urlencoded::Serializer::new(String::new())
            .extend_pairs(fields)
            .finish();
        let request = self
            .client
            .post(format!("{}{path}", self.base))
            .header(CONTENT_TYPE, "application/x-www-form-urlencoded")
            .body(body);
        self.send(Method::POST, path, request).await
    }

    pub async fn sign_in(&mut self, name: &str, password: &str) -> Page {
        let form = self.get("/admin/sign-in").await;
        self.post_form(
            "/admin/sign-in",
            &[
                ("csrf", &form.csrf_token()),
                ("name", name),
                ("password", password),
            ],
        )
        .await
    }

    pub async fn sign_out(&mut self) -> Page {
        let home = self.get("/admin").await;
        self.post_form("/admin/sign-out", &[("csrf", &home.csrf_token())])
            .await
    }

    pub fn cookie(&self, name: &str) -> Option<&str> {
        self.cookies.get(name).map(String::as_str)
    }

    pub fn set_cookie(&mut self, name: &str, value: &str) {
        self.cookies.insert(name.to_owned(), value.to_owned());
    }

    pub fn set_header(&mut self, name: &'static str, value: &'static str) {
        self.headers.insert(name, HeaderValue::from_static(value));
    }

    async fn send(&mut self, method: Method, path: &str, mut request: RequestBuilder) -> Page {
        request = request.headers(self.headers.clone());
        if !self.cookies.is_empty() {
            let header = self
                .cookies
                .iter()
                .map(|(name, value)| format!("{name}={value}"))
                .collect::<Vec<_>>()
                .join("; ");
            request = request.header(COOKIE, header);
        }
        let response = request.send().await.unwrap();
        let set_cookies = response
            .headers()
            .get_all(SET_COOKIE)
            .iter()
            .map(|x| x.to_str().unwrap().to_owned())
            .collect::<Vec<_>>();
        for set_cookie in &set_cookies {
            let (pair, attributes) = set_cookie.split_once(';').unwrap_or((set_cookie, ""));
            let (name, value) = pair.split_once('=').unwrap();
            if attributes.contains("Max-Age=0") {
                self.cookies.remove(name);
            } else {
                self.cookies.insert(name.to_owned(), value.to_owned());
            }
        }
        let status = response.status();
        let location = response
            .headers()
            .get(LOCATION)
            .map(|x| x.to_str().unwrap().to_owned());
        let content_type = media_type(&response);
        let body = response.text().await.unwrap();
        if content_type.as_deref().is_some_and(|x| x.ends_with("json")) {
            openapi::check_response(
                method.as_str(),
                path,
                status.as_u16(),
                content_type.as_deref(),
                body.as_bytes(),
            );
        }
        Page {
            status,
            location,
            set_cookies,
            body,
        }
    }
}
