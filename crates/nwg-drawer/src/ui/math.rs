use gtk4::prelude::*;

/// Result of attempting to evaluate a math expression.
#[derive(Debug)]
pub enum MathResult {
    /// Successfully evaluated to a numeric result.
    Value(f64),
    /// Evaluated but produced a runtime error (e.g. division by zero, overflow).
    /// Parse failures go to `NotMath` — incomplete expressions while typing are not shown.
    Error(String),
    /// Not a math expression — just a search query.
    NotMath,
}

/// Evaluates a math expression using the meval crate.
/// Supports: +, -, *, /, ^, %, parentheses, decimals,
/// functions (sin, cos, sqrt, abs, ln, log, etc.),
/// and constants (pi, e).
pub fn eval_expression(expr: &str) -> MathResult {
    let trimmed = expr.trim();
    if trimmed.is_empty() {
        return MathResult::NotMath;
    }
    match meval::eval_str(trimmed) {
        Ok(val) if val.is_nan() => MathResult::Error("undefined".to_string()),
        Ok(val) if val.is_infinite() => MathResult::Error("overflow".to_string()),
        Ok(val) => MathResult::Value(val),
        Err(_) => MathResult::NotMath,
    }
}

/// Builds an inline math result widget for the search well.
/// Returns `None` if the phrase isn't a math expression.
pub fn build_math_result(phrase: &str) -> Option<gtk4::Box> {
    let (label_text, result_str) = match eval_expression(phrase) {
        MathResult::Value(val) => {
            let r = format_result(val);
            (format!("{} = {}", phrase, r), Some(r))
        }
        MathResult::Error(msg) => (format!("{} — {}", phrase, msg), None),
        MathResult::NotMath => return None,
    };

    let vbox = gtk4::Box::new(
        gtk4::Orientation::Vertical,
        super::constants::MATH_VBOX_SPACING as i32,
    );
    vbox.set_halign(gtk4::Align::Center);
    vbox.set_margin_top(super::constants::STATUS_AREA_VERTICAL_MARGIN);
    vbox.set_margin_bottom(super::constants::STATUS_AREA_VERTICAL_MARGIN);

    let row = gtk4::Box::new(gtk4::Orientation::Horizontal, 0);
    row.set_halign(gtk4::Align::Center);

    let label = gtk4::Label::new(Some(&label_text));
    label.add_css_class("math-result");
    label.set_halign(gtk4::Align::End);
    row.append(&label);

    // Copy button — copies the result to clipboard via wl-copy.
    // "Copied!" confirmation shown below, auto-hides after 2 seconds.
    if let Some(result_copy) = result_str {
        let sep = gtk4::Separator::new(gtk4::Orientation::Vertical);
        sep.add_css_class("math-divider");
        row.append(&sep);

        let copy_btn = gtk4::Button::with_label("Copy");
        copy_btn.add_css_class("math-copy");
        copy_btn.set_focusable(true);
        copy_btn.set_halign(gtk4::Align::Start);

        let copied_label = gtk4::Label::new(Some("Copied!"));
        copied_label.add_css_class("math-copied");
        copied_label.set_visible(false);

        let copied_ref = copied_label.clone();
        let pending_timer: std::rc::Rc<std::cell::Cell<Option<gtk4::glib::SourceId>>> =
            std::rc::Rc::new(std::cell::Cell::new(None));
        let timer_ref = std::rc::Rc::clone(&pending_timer);
        copy_btn.connect_clicked(move |_| {
            // Cancel previous hide timer so repeated clicks reset the 2s window
            if let Some(id) = timer_ref.take() {
                id.remove();
            }
            // Only show "Copied!" if wl-copy actually started
            if std::process::Command::new("wl-copy")
                .arg(&result_copy)
                .spawn()
                .is_err()
            {
                return;
            }
            copied_ref.set_visible(true);
            let hide_ref = copied_ref.clone();
            let timer_reset = std::rc::Rc::clone(&timer_ref);
            let id = gtk4::glib::timeout_add_local_once(
                std::time::Duration::from_secs(2),
                move || {
                    hide_ref.set_visible(false);
                    timer_reset.set(None);
                },
            );
            timer_ref.set(Some(id));
        });
        row.append(&copy_btn);
        vbox.append(&row);
        vbox.append(&copied_label);
    } else {
        vbox.append(&row);
    }

    // Load math CSS once (dimensions from ui/constants.rs)
    use super::constants::*;
    static CSS_LOADED: std::sync::Once = std::sync::Once::new();
    CSS_LOADED.call_once(|| {
        let provider = gtk4::CssProvider::new();
        provider.load_from_data(&format!(
            ".math-result {{ font-size: {fs}px; font-weight: bold; margin-right: {sp}px; }} \
             .math-divider {{ margin-left: 0px; margin-right: 0px; }} \
             .math-copy {{ font-size: {fs}px; background: #5b9bd5; color: white; border-radius: {br}px; padding: {pv}px {ph}px; margin-left: {sp}px; }} \
             .math-copy:hover {{ background: #4a8bc2; }} \
             .math-copied {{ color: #5b9bd5; font-style: italic; }}",
            fs = MATH_FONT_SIZE,
            sp = MATH_SPACING,
            br = MATH_BORDER_RADIUS,
            pv = MATH_BUTTON_PADDING_V,
            ph = MATH_BUTTON_PADDING_H,
        ));
        if let Some(display) = gtk4::gdk::Display::default() {
            gtk4::style_context_add_provider_for_display(
                &display,
                &provider,
                gtk4::STYLE_PROVIDER_PRIORITY_APPLICATION,
            );
        }
    });

    Some(vbox)
}

fn format_result(value: f64) -> String {
    // Show integers without decimal point (up to i64 safe range)
    if value == value.floor() && value.abs() < 1e15 {
        format!("{}", value as i64)
    } else if value.abs() >= 1e15 || (value != 0.0 && value.abs() < 1e-4) {
        // Scientific notation for very large or very small numbers
        format!("{:.6e}", value)
            .trim_end_matches('0')
            .trim_end_matches('.')
            .to_string()
    } else {
        // 6 decimal places, trailing zeros stripped for clean display
        let formatted = format!("{:.6}", value)
            .trim_end_matches('0')
            .trim_end_matches('.')
            .to_string();
        // Normalize -0 to 0 (e.g. sin(-pi) rounds to -0)
        if formatted == "-0" { "0".to_string() } else { formatted }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn eval_val(expr: &str) -> f64 {
        match eval_expression(expr) {
            MathResult::Value(v) => v,
            _ => panic!("expected Value for '{}'", expr),
        }
    }

    #[test]
    fn basic_arithmetic() {
        assert_eq!(eval_val("2+2"), 4.0);
        assert_eq!(eval_val("10 - 3"), 7.0);
        assert_eq!(eval_val("6 * 7"), 42.0);
        assert_eq!(eval_val("10 / 4"), 2.5);
    }

    #[test]
    fn operator_precedence() {
        assert_eq!(eval_val("2 + 3 * 4"), 14.0);
        assert_eq!(eval_val("(2 + 3) * 4"), 20.0);
    }

    #[test]
    fn negative_numbers() {
        assert_eq!(eval_val("-5"), -5.0);
        assert_eq!(eval_val("3 + -2"), 1.0);
    }

    #[test]
    fn division_by_zero() {
        assert!(matches!(eval_expression("1/0"), MathResult::Error(_)));
    }

    #[test]
    fn not_math() {
        assert!(matches!(eval_expression("firefox"), MathResult::NotMath));
        assert!(matches!(eval_expression(""), MathResult::NotMath));
        assert!(matches!(eval_expression("   "), MathResult::NotMath));
    }

    #[test]
    fn incomplete_expression_is_not_math() {
        // Incomplete expressions while typing should not show inline errors
        assert!(matches!(eval_expression("2+"), MathResult::NotMath));
        assert!(matches!(eval_expression("(3*"), MathResult::NotMath));
        assert!(matches!(eval_expression("sqrt("), MathResult::NotMath));
    }

    #[test]
    fn overflow_not_div_by_zero() {
        // 2^1024 overflows to infinity — should say "overflow", not "division by zero"
        match eval_expression("2^1024") {
            MathResult::Error(msg) => assert_eq!(msg, "overflow"),
            other => panic!("expected Error(overflow), got {:?}", other),
        }
    }

    #[test]
    #[allow(clippy::approx_constant)]
    fn decimals() {
        assert!((eval_val("3.14 * 2") - 6.28).abs() < 1e-10);
    }

    #[test]
    fn nested_parens() {
        assert_eq!(eval_val("(((1 + 2)))"), 3.0);
    }

    #[test]
    fn power_operator() {
        assert_eq!(eval_val("2^10"), 1024.0);
    }

    #[test]
    fn builtin_functions() {
        assert_eq!(eval_val("sqrt(16)"), 4.0);
        assert_eq!(eval_val("abs(-5)"), 5.0);
    }

    #[test]
    fn builtin_constants() {
        assert!((eval_val("pi") - std::f64::consts::PI).abs() < 1e-10);
        assert!((eval_val("e") - std::f64::consts::E).abs() < 1e-10);
    }

    #[test]
    fn format_integer() {
        assert_eq!(format_result(42.0), "42");
    }

    #[test]
    fn format_decimal() {
        assert_eq!(format_result(3.14), "3.14");
    }

    #[test]
    fn undefined_nan() {
        // sqrt(-1) produces NaN — should show "undefined"
        match eval_expression("sqrt(-1)") {
            MathResult::Error(msg) => assert_eq!(msg, "undefined"),
            _ => panic!("expected Error(undefined)"),
        }
    }

    #[test]
    fn negative_zero_normalized() {
        // -0.0 displays as "0" via integer branch
        assert_eq!(format_result(-0.0), "0");
        // Very small negative (like sin(-pi)) uses scientific notation, not "-0"
        assert_ne!(format_result(-1.2e-16), "-0");
    }

    #[test]
    fn format_large_number_uses_scientific() {
        let result = format_result(2.0f64.powi(1023));
        assert!(result.contains('e'), "expected scientific notation, got: {}", result);
    }

    #[test]
    fn format_tiny_number_uses_scientific() {
        let result = format_result(0.00001);
        assert!(result.contains('e'), "expected scientific notation, got: {}", result);
    }
}
