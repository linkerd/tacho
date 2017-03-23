use chrono::datetime::DateTime;
use chrono::offset::local::Local;

#[derive(Clone, Debug)]
pub struct Timer {
    pub name: String,
    pub started_at: Option<DateTime<Local>>,
    // Time in milliseconds between started_at and when the recorded action ended.
    pub elapsed: Option<u64>,
}

impl Timer {
    pub fn new(name: String) -> Timer {
        Timer {
            name: name,
            started_at: None,
            elapsed: None,
        }
    }

    pub fn start(&mut self) {
        self.started_at = Some(Local::now());
    }

    pub fn stop(&mut self) {
        if let Some(then) = self.started_at {
            let now = Local::now();
            // Warning: this only record milliseconds.
            let elapsed = now.signed_duration_since(then).num_microseconds();
            match elapsed {
                Some(micros) => self.elapsed = Some(micros as u64),
                None => println!("WARNING: overflow happened for metric {}", self.name),
            }
        }
    }

    pub fn fresh(&self) -> Timer {
        Timer {
            name: self.name.clone(),
            started_at: None,
            elapsed: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_basic_timing() {
        // mut needed because the main functions are defined as &mut self
        let _ = Timer::new("foo".to_owned());
    }
}
