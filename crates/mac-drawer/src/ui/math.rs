use gtk4::prelude::*;
use gtk4_layer_shell::LayerShell;

/// Evaluates a basic arithmetic expression.
/// Supports +, -, *, /, parentheses, and decimal numbers.
pub fn eval_expression(expr: &str) -> Option<f64> {
    let mut parser = Parser::new(expr);
    let result = parser.parse_expr();
    if parser.is_done() { Some(result?) } else { None }
}

/// Shows a math result popup window and copies result to clipboard.
pub fn show_result_window(
    expression: &str,
    result: f64,
    app: &gtk4::Application,
) -> gtk4::ApplicationWindow {
    let window = gtk4::ApplicationWindow::new(app);
    window.init_layer_shell();
    window.set_layer(gtk4_layer_shell::Layer::Overlay);
    window.set_keyboard_mode(gtk4_layer_shell::KeyboardMode::Exclusive);

    let result_str = format_result(result);
    let label_text = format!("{} = {}", expression, result_str);

    let vbox = gtk4::Box::new(gtk4::Orientation::Vertical, 6);
    vbox.set_margin_top(12);
    vbox.set_margin_bottom(12);
    vbox.set_margin_start(24);
    vbox.set_margin_end(24);

    let label = gtk4::Label::new(Some(&label_text));
    label.set_widget_name("math-label");
    label.set_selectable(true);
    vbox.append(&label);
    window.set_child(Some(&vbox));

    // Style
    let provider = gtk4::CssProvider::new();
    provider.load_from_data(
        "window { background-color: rgba(0, 0, 0, 0.9); color: #fff; border: solid 1px grey; border-radius: 5px; }"
    );
    let display = gtk4::gdk::Display::default().expect("No display");
    gtk4::style_context_add_provider_for_display(
        &display,
        &provider,
        gtk4::STYLE_PROVIDER_PRIORITY_APPLICATION + 1,
    );

    // Close on any key
    let win_ref = window.clone();
    let key_ctrl = gtk4::EventControllerKey::new();
    key_ctrl.connect_key_released(move |_, _, _, _| {
        win_ref.close();
    });
    window.add_controller(key_ctrl);

    // Close on click
    let win_ref = window.clone();
    let click = gtk4::GestureClick::new();
    click.connect_released(move |_, _, _, _| {
        win_ref.close();
    });
    window.add_controller(click);

    window.present();

    // Copy to clipboard via wl-copy
    let _ = std::process::Command::new("wl-copy")
        .arg(&result_str)
        .spawn();

    window
}

fn format_result(value: f64) -> String {
    if value == value.floor() && value.abs() < 1e15 {
        format!("{}", value as i64)
    } else {
        format!("{}", value)
    }
}

// --- Recursive descent parser for arithmetic expressions ---

struct Parser {
    chars: Vec<char>,
    pos: usize,
}

impl Parser {
    fn new(input: &str) -> Self {
        Self {
            chars: input.chars().collect(),
            pos: 0,
        }
    }

    fn is_done(&self) -> bool {
        self.skip_spaces_pos() >= self.chars.len()
    }

    fn skip_spaces_pos(&self) -> usize {
        let mut p = self.pos;
        while p < self.chars.len() && self.chars[p].is_whitespace() {
            p += 1;
        }
        p
    }

    fn skip_spaces(&mut self) {
        while self.pos < self.chars.len() && self.chars[self.pos].is_whitespace() {
            self.pos += 1;
        }
    }

    fn peek(&mut self) -> Option<char> {
        self.skip_spaces();
        self.chars.get(self.pos).copied()
    }

    fn consume(&mut self) -> Option<char> {
        self.skip_spaces();
        if self.pos < self.chars.len() {
            let c = self.chars[self.pos];
            self.pos += 1;
            Some(c)
        } else {
            None
        }
    }

    fn parse_expr(&mut self) -> Option<f64> {
        let mut left = self.parse_term()?;
        loop {
            match self.peek() {
                Some('+') => { self.consume(); left += self.parse_term()?; }
                Some('-') => { self.consume(); left -= self.parse_term()?; }
                _ => return Some(left),
            }
        }
    }

    fn parse_term(&mut self) -> Option<f64> {
        let mut left = self.parse_unary()?;
        loop {
            match self.peek() {
                Some('*') => { self.consume(); left *= self.parse_unary()?; }
                Some('/') => {
                    self.consume();
                    let right = self.parse_unary()?;
                    if right == 0.0 { return None; }
                    left /= right;
                }
                _ => return Some(left),
            }
        }
    }

    fn parse_unary(&mut self) -> Option<f64> {
        match self.peek() {
            Some('-') => { self.consume(); Some(-self.parse_primary()?) }
            Some('+') => { self.consume(); self.parse_primary() }
            _ => self.parse_primary(),
        }
    }

    fn parse_primary(&mut self) -> Option<f64> {
        if self.peek() == Some('(') {
            self.consume(); // '('
            let val = self.parse_expr()?;
            if self.peek() == Some(')') { self.consume(); }
            Some(val)
        } else {
            self.parse_number()
        }
    }

    fn parse_number(&mut self) -> Option<f64> {
        self.skip_spaces();
        let start = self.pos;
        let mut has_dot = false;

        while self.pos < self.chars.len() {
            match self.chars[self.pos] {
                c if c.is_ascii_digit() => self.pos += 1,
                '.' if !has_dot => {
                    has_dot = true;
                    self.pos += 1;
                }
                _ => break,
            }
        }

        if self.pos == start {
            return None;
        }
        let s: String = self.chars[start..self.pos].iter().collect();
        s.parse().ok()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn basic_arithmetic() {
        assert_eq!(eval_expression("2+2"), Some(4.0));
        assert_eq!(eval_expression("10 - 3"), Some(7.0));
        assert_eq!(eval_expression("6 * 7"), Some(42.0));
        assert_eq!(eval_expression("10 / 4"), Some(2.5));
    }

    #[test]
    fn operator_precedence() {
        assert_eq!(eval_expression("2 + 3 * 4"), Some(14.0));
        assert_eq!(eval_expression("(2 + 3) * 4"), Some(20.0));
    }

    #[test]
    fn negative_numbers() {
        assert_eq!(eval_expression("-5"), Some(-5.0));
        assert_eq!(eval_expression("3 + -2"), Some(1.0));
    }

    #[test]
    fn division_by_zero() {
        assert_eq!(eval_expression("1/0"), None);
    }

    #[test]
    fn invalid_expression() {
        assert_eq!(eval_expression("abc"), None);
        assert_eq!(eval_expression(""), None);
    }

    #[test]
    #[allow(clippy::approx_constant)]
    fn decimals() {
        let result = eval_expression("3.14 * 2").unwrap();
        assert!((result - 6.28).abs() < 1e-10);
    }
}
