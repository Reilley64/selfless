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
use reqwest::header::ORIGIN;
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
        Browser::new(self.url(""))
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
        headers: HeaderMap,
        body: Option<Value>,
    ) -> (StatusCode, Value) {
        let mut request = reqwest::Client::new()
            .request(method.clone(), self.url(path))
            .headers(headers);
        if let Some(token) = token {
            request = request.header(AUTHORIZATION, format!("Bearer {token}"));
        }
        if let Some(body) = body {
            request = request.json(&body);
        }
        let response = request.send().await.unwrap();
        let status = response.status();
        let content_type = media_type(&response);
        let text = response.text().await.unwrap();
        let body = openapi::check_response(
            method.as_str(),
            path,
            status.as_u16(),
            content_type.as_deref(),
            &text,
        );
        (status, body)
    }

    pub async fn get(&self, path: &str) -> (StatusCode, Value) {
        self.request(Method::GET, path, None, None).await
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

    pub fn base_url(&self) -> String {
        self.url("")
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
    pub headers: HeaderMap,
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

    pub fn text(&self) -> String {
        visible_text(&self.body)
    }

    pub fn table_rows(&self) -> Vec<Vec<String>> {
        self.body
            .split("<tr")
            .skip(1)
            .map(|row| {
                row[..row.find("</tr>").unwrap()]
                    .split("<td")
                    .skip(1)
                    .map(|cell| {
                        let start = cell.find('>').unwrap() + 1;
                        visible_text(&cell[start..cell.find("</td>").unwrap()])
                    })
                    .collect::<Vec<_>>()
            })
            .filter(|x| !x.is_empty())
            .collect()
    }

    pub fn link(&self, text: &str) -> String {
        let text_at = self
            .body
            .find(&format!(">{text}<"))
            .unwrap_or_else(|| panic!("the page has no {text:?} link\n{}", self.body));
        let link_at = self.body[..text_at]
            .rfind("<a ")
            .unwrap_or_else(|| panic!("{text:?} is not in a link\n{}", self.body));
        let start = link_at + self.body[link_at..].find(r#"href=""#).unwrap() + r#"href=""#.len();
        let length = self.body[start..].find('"').unwrap();
        self.body[start..start + length].replace("&amp;", "&")
    }

    pub fn header(&self, name: &str) -> Option<&str> {
        self.headers.get(name).map(|x| x.to_str().unwrap())
    }

    pub fn form_action(&self, button: &str) -> String {
        let button_at = self
            .body
            .find(&format!(">{button}<"))
            .unwrap_or_else(|| panic!("the page has no {button:?} button\n{}", self.body));
        let form_at = self.body[..button_at]
            .rfind("<form")
            .unwrap_or_else(|| panic!("{button:?} is not in a form\n{}", self.body));
        let start =
            form_at + self.body[form_at..].find(r#"action=""#).unwrap() + r#"action=""#.len();
        let length = self.body[start..].find('"').unwrap();
        self.body[start..start + length].to_owned()
    }

    pub fn htmx_csrf_token(&self) -> String {
        let start = self
            .body
            .find(r#"hx-headers=""#)
            .unwrap_or_else(|| panic!("the page sets no htmx headers\n{}", self.body))
            + r#"hx-headers=""#.len();
        let length = self.body[start..].find('"').unwrap();
        let headers: Value =
            serde_json::from_str(&self.body[start..start + length].replace("&quot;", "\""))
                .unwrap();
        headers["X-CSRF-Token"].as_str().unwrap().to_owned()
    }

    pub fn section_text(&self, label: &str) -> String {
        let label_at = self
            .body
            .find(&format!(r#"aria-label="{label}""#))
            .unwrap_or_else(|| panic!("the page has no {label:?} section\n{}", self.body));
        let start = self.body[..label_at].rfind("<section").unwrap();
        let end = start + self.body[start..].find("</section>").unwrap();
        visible_text(&self.body[start..end])
    }

    pub fn hx_post(&self, button: &str) -> String {
        let tag = self.opening_tag("button", button);
        attribute(tag, "hx-post").unwrap_or_else(|| panic!("{button:?} posts nothing: {tag}"))
    }

    pub fn hx_get_on_choosing(&self, option: &str) -> String {
        let option_tag = self.opening_tag("option", option);
        let option_at = self.body.find(option_tag).unwrap();
        let select_at = self.body[..option_at].rfind("<select").unwrap();
        let select_tag = &self.body[select_at..select_at + tag_length(&self.body[select_at..])];
        let path = attribute(select_tag, "hx-get")
            .unwrap_or_else(|| panic!("choosing {option:?} gets nothing: {select_tag}"));
        let query = url::form_urlencoded::Serializer::new(String::new())
            .append_pair(
                &attribute(select_tag, "name").unwrap(),
                &attribute(option_tag, "value").unwrap(),
            )
            .finish();
        format!("{path}?{query}")
    }

    fn opening_tag(&self, element: &str, label: &str) -> &str {
        let label_at = self
            .body
            .find(&format!(">{label}<"))
            .unwrap_or_else(|| panic!("the page has no {label:?} {element}\n{}", self.body));
        let start = self.body[..label_at]
            .rfind(&format!("<{element} "))
            .unwrap_or_else(|| panic!("{label:?} is not in a {element}\n{}", self.body));
        &self.body[start..label_at]
    }
}

fn tag_length(html: &str) -> usize {
    let mut quoted = false;
    html.find(|x| {
        if x == '"' {
            quoted = !quoted;
        }
        x == '>' && !quoted
    })
    .unwrap()
}

fn attribute(tag: &str, name: &str) -> Option<String> {
    let start = tag.find(&format!(r#" {name}=""#))? + name.len() + 3;
    let length = tag[start..].find('"').unwrap();
    Some(tag[start..start + length].replace("&amp;", "&"))
}

fn visible_text(html: &str) -> String {
    let mut text = String::new();
    let mut in_tag = false;
    for char in html.chars() {
        match char {
            '<' => in_tag = true,
            '>' => in_tag = false,
            _ if !in_tag => text.push(char),
            _ => {}
        }
    }
    text
}

impl Browser {
    pub fn new(base: String) -> Self {
        Self {
            client: reqwest::Client::builder()
                .redirect(Policy::none())
                .build()
                .unwrap(),
            base,
            cookies: BTreeMap::new(),
            headers: HeaderMap::new(),
        }
    }

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
            .header(ORIGIN, &self.base)
            .header(CONTENT_TYPE, "application/x-www-form-urlencoded")
            .body(body);
        self.send(Method::POST, path, request).await
    }

    pub async fn htmx_post(&mut self, path: &str, csrf: Option<&str>) -> Page {
        let mut request = self
            .client
            .post(format!("{}{path}", self.base))
            .header("HX-Request", "true")
            .header(ORIGIN, &self.base)
            .header(CONTENT_TYPE, "application/x-www-form-urlencoded")
            .body("");
        if let Some(csrf) = csrf {
            request = request.header("X-CSRF-Token", csrf);
        }
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
        let form = self.get("/admin/sign-in").await;
        self.post_form("/admin/sign-out", &[("csrf", &form.csrf_token())])
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
        let headers = response.headers().clone();
        let content_type = media_type(&response);
        let body = response.text().await.unwrap();
        if content_type.as_deref().is_some_and(|x| x.ends_with("json")) {
            openapi::check_response(
                method.as_str(),
                path,
                status.as_u16(),
                content_type.as_deref(),
                &body,
            );
        }
        Page {
            status,
            location,
            set_cookies,
            headers,
            body,
        }
    }
}
