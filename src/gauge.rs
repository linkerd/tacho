struct Gauge {
    name: String
    value: u64
}

impl Gauge {
    fn new(name: String, value: u64) -> Gauge {
        Gauge {
            name: name,
            value: value
        }
    }
}

#[cfg(test)]
mod tests {
    use gauge;
    use timer::Gauge;

    #[test]
    fn test_basic_gauges() {
        let v = Gauge::new("foo", 1);
        assert_eq!(v.value, 1)
    }
}
