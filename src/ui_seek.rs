use cursive::{
    event::{Event, EventResult, Key},
    Printer, Vec2, View,
};

#[derive(Copy, Clone)]
enum TimeField {
    H,
    M,
    S,
}

impl TimeField {
    fn usize(&self) -> usize {
        match self {
            Self::H => 0,
            Self::M => 1,
            Self::S => 2,
        }
    }
    fn next(&self) -> Self {
        match self {
            Self::H => panic!("Cannot next H"),
            Self::M => Self::H,
            Self::S => Self::M,
        }
    }
    fn prev(&self) -> Option<Self> {
        match self {
            Self::H => Some(Self::M),
            Self::M => Some(Self::S),
            Self::S => None,
        }
    }
}

enum TimeDirection {
    Backwards,
    Forwards,
}

enum EntryDirection {
    ScrollsToLeft,
    FillsToRight,
}

struct Time {
    time_direction: TimeDirection,
    entry_direction: EntryDirection,
    field: TimeField,
    hhmmss: Vec<String>,
}

pub struct UiSeek {
    time: Time, // [0] = hh, [1] == mm, [2] == ss
    callback: Option<Box<dyn FnOnce(i32)>>,
}

fn overflow_digit(hhmmss: &mut Vec<String>, idx: TimeField, digit: char) -> bool {
    if hhmmss[idx.usize()].len() == 2 {
        if idx.usize() == 0 {
            false
        } else {
            if overflow_digit(
                hhmmss,
                idx.next(),
                hhmmss[idx.usize()].chars().nth(0).unwrap(),
            ) {
                hhmmss[idx.usize()] =
                    format!("{}{}", hhmmss[idx.usize()].chars().nth(1).unwrap(), digit);
                true
            } else {
                false
            }
        }
    } else {
        hhmmss[idx.usize()] = format!("{}{}", hhmmss[idx.usize()], digit);
        true
    }
}

impl Time {
    fn new(initial_digit: char) -> Time {
        Time {
            hhmmss: vec![
                String::from(""),
                String::from(""),
                if initial_digit == '-' {
                    String::from("")
                } else {
                    initial_digit.to_string()
                },
            ],
            field: TimeField::S,
            time_direction: if initial_digit == '-' {
                TimeDirection::Backwards
            } else {
                TimeDirection::Forwards
            },
            entry_direction: EntryDirection::ScrollsToLeft,
        }
    }

    fn add_digit(&mut self, digit: char) -> bool {
        if self.hhmmss[self.field.usize()].len() == 2 {
            match self.entry_direction {
                EntryDirection::ScrollsToLeft => {
                    overflow_digit(&mut self.hhmmss, self.field, digit)
                }
                EntryDirection::FillsToRight => {
                    if self.field.usize() > 0 {
                        self.field = self.field.prev().unwrap_or(TimeField::S);
                        self.hhmmss[self.field.usize()] = digit.to_string();
                        true
                    } else {
                        false
                    }
                }
            }
        } else {
            self.hhmmss[self.field.usize()] =
                format!("{}{}", self.hhmmss[self.field.usize()], digit);
            true
        }
    }

    fn flip_direction(&mut self) {
        self.time_direction = match self.time_direction {
            TimeDirection::Forwards => TimeDirection::Backwards,
            TimeDirection::Backwards => TimeDirection::Forwards,
        }
    }

    fn enter_multiplier(&mut self, field: TimeField) {
        // what if you're in hours but then enter seconds..
        self.hhmmss[field.usize()] = self.hhmmss[self.field.usize()].clone();
        self.field = field.prev().unwrap_or(TimeField::S);
        for idx in self.field.usize()..=2 {
            self.hhmmss[idx] = "".to_string();
        }
        self.entry_direction = EntryDirection::FillsToRight;
    }

    fn seconds(&self) -> i32 {
        let mut seconds = 0;
        let multipliers = vec![3600, 60, 1];
        for idx in 0..=2 {
            let value = if self.hhmmss[idx].is_empty() {
                0
            } else {
                self.hhmmss[idx]
                    .parse()
                    .expect("Fields cannot contain unparseable data")
            };
            seconds += multipliers[idx] * value;
        }
        match self.time_direction {
            TimeDirection::Forwards => seconds,
            TimeDirection::Backwards => -seconds,
        }
    }

    fn draw<S: Into<Vec2>>(&self, printer: &Printer<'_, '_>, start: S) {
        let at: Vec2 = start.into();
        // printer.print(at, &self.to_string());
        // printer.print(at + (0, 1), &self.to_string());

        let mut x = 0;
        match self.time_direction {
            TimeDirection::Forwards => {
                printer.print(at + (x, 0), " ");
                x += 1;
            }
            TimeDirection::Backwards => {
                printer.print(at + (x, 0), "-");
                x += 1;
            }
        }
        for idx in 0..=2 {
            if idx >= 1 {
                printer.print(at + (x, 0), ":");
                x += 1;
            }
            let field = &self.hhmmss[idx];
            if idx == self.field.usize() {
                printer.print(at + (x, 1), "--");
            }
            if field.len() < 1 {
                printer.print(at + (x, 0), "0");
                x += 1;
            }
            if field.len() < 2 {
                printer.print(at + (x, 0), "0");
                x += 1;
            }
            printer.print(at + (x, 0), self.hhmmss[idx].as_str());
            x += self.hhmmss[idx].len();
        }
    }
}

impl std::fmt::Display for Time {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self.time_direction {
            TimeDirection::Forwards => write!(f, " ")?,
            TimeDirection::Backwards => write!(f, "-")?,
        }
        for idx in 0..=2 {
            if idx >= 1 {
                write!(f, ":")?;
            }
            let field = &self.hhmmss[idx];
            if field.len() < 1 {
                write!(f, "0")?;
            }
            if field.len() < 2 {
                write!(f, "0")?;
            }
            write!(f, "{}", self.hhmmss[idx])?;
        }
        Ok(())
    }
}

impl UiSeek {
    pub fn new(initial_digit: char) -> UiSeek {
        UiSeek {
            time: Time::new(initial_digit),
            callback: None,
        }
    }

    pub fn set_callback<F>(mut self, callback: F) -> UiSeek
    where
        F: FnOnce(i32) + 'static,
    {
        self.callback = Some(Box::new(callback));
        self
    }
}

impl View for UiSeek {
    fn draw(&self, printer: &Printer<'_, '_>) {
        self.time.draw(&printer, ((20 - 6) / 2, 2));
    }

    fn required_size(&mut self, _constraint: Vec2) -> Vec2 {
        Vec2::new(20, 4)
    }

    fn on_event(&mut self, event: Event) -> EventResult {
        match event {
            Event::Char(digit) if digit >= '0' && digit <= '9' => {
                if !self.time.add_digit(digit) {
                    EventResult::with_cb(|siv| {
                        let _ = siv.pop_layer();
                    })
                } else {
                    EventResult::Consumed(None)
                }
            }
            Event::Char('s') => {
                self.time.enter_multiplier(TimeField::S);
                EventResult::Consumed(None)
            }
            Event::Char('m') => {
                self.time.enter_multiplier(TimeField::M);
                EventResult::Consumed(None)
            }
            Event::Char('h') => {
                self.time.enter_multiplier(TimeField::H);
                EventResult::Consumed(None)
            }
            Event::Char('-') => {
                self.time.flip_direction();
                EventResult::Consumed(None)
            }
            Event::Key(Key::Enter) => {
                match self.callback.take() {
                    None => (),
                    Some(callback) => callback(self.time.seconds()),
                }
                EventResult::with_cb(|siv| {
                    let _ = siv.pop_layer();
                })
            }
            _ => EventResult::Ignored,
        }
    }
}
