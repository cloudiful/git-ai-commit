use reqwest::Response;
use std::error::Error;
use std::time::Duration;

const ERROR_CHAIN_LIMIT: usize = 8;
const HEADER_VALUE_PREVIEW_LIMIT: usize = 240;
const DIAGNOSTIC_HEADERS: [&str; 9] = [
    "content-length",
    "transfer-encoding",
    "content-encoding",
    "connection",
    "server",
    "via",
    "x-request-id",
    "x-correlation-id",
    "cf-ray",
];

pub(super) fn response_context(response: &Response) -> Vec<String> {
    let mut context = vec![format!("http-version: {:?}", response.version())];

    if let Some(remote_addr) = response.remote_addr() {
        context.push(format!("remote-addr: {remote_addr}"));
    }

    for name in DIAGNOSTIC_HEADERS {
        let values = response
            .headers()
            .get_all(name)
            .iter()
            .filter_map(|value| value.to_str().ok())
            .map(preview_header_value)
            .collect::<Vec<_>>();
        if !values.is_empty() {
            context.push(format!("{name}: {}", values.join(", ")));
        }
    }

    context
}

pub(super) fn describe_reqwest_error(error: &reqwest::Error) -> String {
    format!(
        "{} [timeout={}, connect={}, request={}, body={}, decode={}]",
        describe_error_chain(error),
        error.is_timeout(),
        error.is_connect(),
        error.is_request(),
        error.is_body(),
        error.is_decode()
    )
}

pub(super) fn format_duration(duration: Duration) -> String {
    if duration.as_secs() > 0 {
        format!("{:.3}s", duration.as_secs_f64())
    } else {
        format!("{}ms", duration.as_millis())
    }
}

fn describe_error_chain(error: &dyn Error) -> String {
    let mut messages = vec![error.to_string()];
    let mut source = error.source();

    for _ in 0..ERROR_CHAIN_LIMIT {
        let Some(cause) = source else {
            break;
        };
        let message = cause.to_string();
        if messages.last() != Some(&message) {
            messages.push(message);
        }
        source = cause.source();
    }

    messages.join("; caused by: ")
}

fn preview_header_value(value: &str) -> String {
    let compact = value.split_whitespace().collect::<Vec<_>>().join(" ");
    let mut chars = compact.chars();
    let preview = chars
        .by_ref()
        .take(HEADER_VALUE_PREVIEW_LIMIT)
        .collect::<String>();
    if chars.next().is_some() {
        format!("{preview}...")
    } else {
        preview
    }
}

#[cfg(test)]
mod tests {
    use super::describe_error_chain;
    use std::error::Error;
    use std::fmt;
    use std::io;

    #[derive(Debug)]
    struct BodyReadError {
        source: io::Error,
    }

    impl fmt::Display for BodyReadError {
        fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
            formatter.write_str("response body read failed")
        }
    }

    impl Error for BodyReadError {
        fn source(&self) -> Option<&(dyn Error + 'static)> {
            Some(&self.source)
        }
    }

    #[test]
    fn includes_nested_error_causes() {
        let body = BodyReadError {
            source: io::Error::new(io::ErrorKind::ConnectionReset, "peer reset stream"),
        };

        assert_eq!(
            describe_error_chain(&body),
            "response body read failed; caused by: peer reset stream"
        );
    }
}
