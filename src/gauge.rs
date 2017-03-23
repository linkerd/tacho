#[derive(Clone, Debug)]
pub struct Gauge {
    pub name: String,
    pub value: u64,
}

impl Gauge {
    pub fn new(name: String, value: u64) -> Gauge {
        Gauge {
            name: name,
            value: value,
        }
    }

    pub fn set(&mut self, value: u64) {
        self.value = value;
    }

    pub fn fresh(&self) -> Gauge {
        Gauge {
            name: self.name.clone(),
            value: 0,
        }
    }
}

#[cfg(test)]
mod tests {
    use gauge::Gauge;

    #[test]
    fn test_basic_gauges() {
        let v = Gauge::new("foo".into(), 1);
        assert_eq!(v.value, 1)
    }
}
