// Runs a bridge's "cookies" sign-in step in a native WebKit window: the user signs in to the
// service normally, and the field values the step asks for (cookies, plus anything the
// step's extract_js resolves) are printed as one JSON object on stdout.
use anyhow::{Context, anyhow};
use serde_json::{Map, Value};
use std::io::Read;
use std::time::{Duration, Instant};
use winit::{
    application::ApplicationHandler,
    event::{StartCause, WindowEvent},
    event_loop::{ActiveEventLoop, ControlFlow, EventLoop, EventLoopProxy},
    platform::macos::{ActivationPolicy, EventLoopBuilderExtMacOS},
    window::{Window, WindowId},
};
use wry::{WebView, WebViewBuilder};

// Services such as Slack reject WKWebView's default user agent as an unsupported browser.
const SAFARI: &str = "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/26.0 Safari/605.1.15";

pub fn run() -> anyhow::Result<()> {
    let mut spec = String::new();
    std::io::stdin().read_to_string(&mut spec)?;
    let spec: Value = serde_json::from_str(&spec).context("Invalid sign-in step")?;
    let event_loop = EventLoop::<Map<String, Value>>::with_user_event()
        .with_activation_policy(ActivationPolicy::Accessory)
        .build()?;
    let mut login = Login {
        proxy: event_loop.create_proxy(),
        fields: spec["fields"].as_array().cloned().unwrap_or_default(),
        spec,
        found: Map::new(),
        window: None,
        webview: None,
        done: false,
    };
    event_loop.run_app(&mut login)?;
    if !login.done {
        return Err(anyhow!(
            "The sign-in window was closed before signing in finished"
        ));
    }
    println!("{}", Value::Object(login.found));
    Ok(())
}

struct Login {
    spec: Value,
    fields: Vec<Value>,
    proxy: EventLoopProxy<Map<String, Value>>,
    found: Map<String, Value>,
    window: Option<Window>,
    webview: Option<WebView>,
    done: bool,
}

impl Login {
    fn accept(&mut self, id: &str, value: &str) {
        let Some(field) = self.fields.iter().find(|f| f["id"] == id) else {
            return;
        };
        let valid = field["pattern"]
            .as_str()
            .and_then(|p| regex::Regex::new(p).ok())
            .is_none_or(|p| p.is_match(value));
        if valid && !value.is_empty() {
            self.found
                .insert(id.to_owned(), Value::String(value.to_owned()));
        }
    }
    fn check(&mut self, event_loop: &ActiveEventLoop) {
        let Some(webview) = &self.webview else { return };
        let cookies = webview.cookies().unwrap_or_default();
        let mut matches = Vec::new();
        for field in &self.fields {
            let Some(id) = field["id"].as_str() else {
                continue;
            };
            for source in field["sources"].as_array().into_iter().flatten() {
                if source["type"] != "cookie" {
                    continue;
                }
                let domain = source["cookie_domain"].as_str().unwrap_or("");
                let cookie = cookies.iter().find(|c| {
                    let d = c.domain().unwrap_or("").trim_start_matches('.');
                    c.name() == source["name"]
                        && (domain.is_empty() || d == domain || d.ends_with(&format!(".{domain}")))
                });
                if let Some(cookie) = cookie {
                    let value = urlencoding::decode(cookie.value())
                        .map(|v| v.into_owned())
                        .unwrap_or_else(|_| cookie.value().to_owned());
                    matches.push((id.to_owned(), value));
                }
            }
        }
        for (id, value) in matches {
            self.accept(&id, &value);
        }
        let complete = self
            .fields
            .iter()
            .filter(|f| f["required"] != false)
            .all(|f| {
                f["id"]
                    .as_str()
                    .is_some_and(|id| self.found.contains_key(id))
            });
        if complete {
            self.done = true;
            event_loop.exit();
        }
    }
}

impl ApplicationHandler<Map<String, Value>> for Login {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.window.is_some() {
            return;
        }
        let attributes = Window::default_attributes()
            .with_title("Sign in")
            .with_inner_size(winit::dpi::LogicalSize::new(560., 760.));
        let window = match event_loop.create_window(attributes) {
            Ok(window) => window,
            Err(error) => {
                eprintln!("Could not open the sign-in window: {error}");
                return event_loop.exit();
            }
        };
        let proxy = self.proxy.clone();
        let mut builder = WebViewBuilder::new()
            .with_url(self.spec["url"].as_str().unwrap_or("about:blank"))
            .with_user_agent(self.spec["user_agent"].as_str().unwrap_or(SAFARI))
            .with_incognito(true)
            .with_ipc_handler(move |request| {
                if let Ok(Value::Object(values)) = serde_json::from_str(request.body()) {
                    let _ = proxy.send_event(values);
                }
            });
        if let Some(js) = self.spec["extract_js"]
            .as_str()
            .filter(|js| !js.trim().is_empty())
        {
            builder = builder.with_initialization_script(format!(
                "Promise.resolve({js}).then(r => window.ipc.postMessage(JSON.stringify(r || {{}})))"
            ));
        }
        match builder.build(&window) {
            Ok(webview) => self.webview = Some(webview),
            Err(error) => {
                eprintln!("Could not open the sign-in page: {error}");
                return event_loop.exit();
            }
        }
        window.focus_window();
        self.window = Some(window);
    }
    fn user_event(&mut self, event_loop: &ActiveEventLoop, values: Map<String, Value>) {
        for (id, value) in values {
            if let Some(value) = value.as_str() {
                self.accept(&id, value);
            }
        }
        self.check(event_loop);
    }
    fn new_events(&mut self, event_loop: &ActiveEventLoop, cause: StartCause) {
        if let StartCause::ResumeTimeReached { .. } = cause {
            self.check(event_loop);
        }
    }
    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        event_loop.set_control_flow(ControlFlow::WaitUntil(
            Instant::now() + Duration::from_secs(1),
        ));
    }
    fn window_event(&mut self, event_loop: &ActiveEventLoop, _: WindowId, event: WindowEvent) {
        if let WindowEvent::CloseRequested = event {
            event_loop.exit();
        }
    }
}
