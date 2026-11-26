//! `JUnitLikeTestReporter`: the `--report` XML of the headless game test server. One `testsuite` inside a `testsuite`,
//! one `testcase` per finished test (a `failure` child for a required failure, `skipped` for an optional one), written as
//! the JDK's identity `Transformer` writes a DOM: one line, attributes sorted by name.

use super::game_test_info::GameTestInfo;
use super::global_test_reporter::TestReporter;
use mc_core::math::java::java_double_to_string;
use std::path::PathBuf;
use std::rc::Rc;
use std::time::Instant;

struct TestCase {
    name: String,
    classname: String,
    time: String,
    /// `("failure" | "skipped", message)`.
    result: Option<(&'static str, String)>,
}

pub struct JUnitLikeTestReporter {
    destination: PathBuf,
    timestamp: String,
    stopwatch: Instant,
    cases: Vec<TestCase>,
}

impl JUnitLikeTestReporter {
    pub fn new(destination: PathBuf) -> Self {
        Self {
            destination,
            timestamp: iso_instant(chrono::Utc::now()),
            stopwatch: Instant::now(),
            cases: Vec::new(),
        }
    }

    /// `createTestCase`: `time` is `getRunTime() / 1000.0` as a Java `double`.
    fn create_test_case(
        test_info: &GameTestInfo,
        result: Option<(&'static str, String)>,
    ) -> TestCase {
        TestCase {
            name: test_info.id().to_string(),
            classname: test_info.get_structure().to_string(),
            time: java_double_to_string(test_info.get_run_time() as f64 / 1000.0),
            result,
        }
    }

    /// `save(file)`.
    fn xml(&self, time: &str) -> String {
        let mut out = String::from(r#"<?xml version="1.0" encoding="UTF-8" standalone="no"?>"#);
        out.push_str("<testsuite><testsuite");
        push_attribute(&mut out, "time", time);
        push_attribute(&mut out, "timestamp", &self.timestamp);
        out.push('>');
        for case in &self.cases {
            out.push_str("<testcase");
            push_attribute(&mut out, "classname", &case.classname);
            push_attribute(&mut out, "name", &case.name);
            push_attribute(&mut out, "time", &case.time);
            match &case.result {
                None => out.push_str("/>"),
                Some((tag, message)) => {
                    out.push_str("><");
                    out.push_str(tag);
                    push_attribute(&mut out, "message", message);
                    out.push_str("/></testcase>");
                }
            }
        }
        out.push_str("</testsuite></testsuite>");
        out
    }
}

impl TestReporter for JUnitLikeTestReporter {
    fn on_test_failed(&mut self, test_info: &Rc<GameTestInfo>) {
        let message = test_info
            .get_error()
            .map(|e| e.get_message())
            .unwrap_or_default();
        let position = test_info
            .get_test_block_pos()
            .map(|p| p.to_short_string())
            .unwrap_or_default();
        let result = (
            if test_info.is_required() {
                "failure"
            } else {
                "skipped"
            },
            format!("({position}) {message}"),
        );
        self.cases
            .push(Self::create_test_case(test_info, Some(result)));
    }

    fn on_test_success(&mut self, test_info: &Rc<GameTestInfo>) {
        self.cases.push(Self::create_test_case(test_info, None));
    }

    fn finish(&mut self) {
        let time = java_double_to_string(self.stopwatch.elapsed().as_millis() as f64 / 1000.0);
        if let Err(e) = std::fs::write(&self.destination, self.xml(&time)) {
            panic!("Couldn't save test report: {e}");
        }
    }
}

/// `DateTimeFormatter.ISO_INSTANT.format`: seconds precision, a 3/6/9 digit fraction only when non-zero.
fn iso_instant(instant: chrono::DateTime<chrono::Utc>) -> String {
    use chrono::{SecondsFormat, Timelike};
    let format = match instant.nanosecond() {
        0 => SecondsFormat::Secs,
        n if n % 1_000_000 == 0 => SecondsFormat::Millis,
        n if n % 1_000 == 0 => SecondsFormat::Micros,
        _ => SecondsFormat::Nanos,
    };
    instant.to_rfc3339_opts(format, true)
}

/// ` name="value"` with the serializer's attribute escapes.
fn push_attribute(out: &mut String, name: &str, value: &str) {
    out.push(' ');
    out.push_str(name);
    out.push_str("=\"");
    for c in value.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            c if (c as u32) < 0x20 => out.push_str(&format!("&#{};", c as u32)),
            c => out.push(c),
        }
    }
    out.push('"');
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_document_is_one_line_with_sorted_attributes() {
        let mut reporter = JUnitLikeTestReporter::new(PathBuf::new());
        reporter.timestamp = "2026-01-02T03:04:05Z".into();
        reporter.cases.push(TestCase {
            name: "a:b".into(),
            classname: "a:b".into(),
            time: "0.05".into(),
            result: None,
        });
        reporter.cases.push(TestCase {
            name: "a:c".into(),
            classname: "a:c".into(),
            time: "1.0".into(),
            result: Some(("failure", "(1, 2, 3) \"x\" <&>\n".into())),
        });
        assert_eq!(
            reporter.xml("2.5"),
            "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"no\"?><testsuite><testsuite time=\"2.5\" \
             timestamp=\"2026-01-02T03:04:05Z\"><testcase classname=\"a:b\" name=\"a:b\" time=\"0.05\"/>\
             <testcase classname=\"a:c\" name=\"a:c\" time=\"1.0\"><failure message=\"(1, 2, 3) \
             &quot;x&quot; &lt;&amp;&gt;&#10;\"/></testcase></testsuite></testsuite>"
        );
    }

    #[test]
    fn an_instant_prints_only_the_fraction_it_has() {
        use chrono::TimeZone;
        let at = |nanos| chrono::Utc.timestamp_opt(1_000_000_000, nanos).unwrap();
        assert_eq!(iso_instant(at(0)), "2001-09-09T01:46:40Z");
        assert_eq!(iso_instant(at(120_000_000)), "2001-09-09T01:46:40.120Z");
        assert_eq!(iso_instant(at(123_456_000)), "2001-09-09T01:46:40.123456Z");
        assert_eq!(iso_instant(at(1)), "2001-09-09T01:46:40.000000001Z");
    }
}
