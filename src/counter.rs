
#[derive(Clone)]
pub struct Counter {
    pub name: String,
    pub value: u64,
}

impl Counter {
    pub fn new(name: String) -> Counter {
        Counter {
            name: name,
            value: 0,
        }
    }

    pub fn incr(&mut self) {
        self.value += 1;
    }

    pub fn fresh(&self) -> Counter {
        Counter {
            name: self.name.clone(),
            value: 0,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::Counter;

    #[test]
    fn incr_works() {
        let mut counter = Counter::new("foo".to_owned());
        counter.incr();
        counter.incr();
        counter.incr();
        assert!(counter.value == 3);
    }
}
