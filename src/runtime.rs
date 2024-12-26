pub mod runtime {
    use std::collections::{ HashMap };
    pub struct Environment {
        env: HashMap<String, String>
    }

    impl Environment{
        pub fn new() -> Environment {
            let mut e = Environment {
                env: HashMap::<String, String>::new()
            };
            e.update(String::from("?"), String::from("0"));
            e
        }

        fn update(&mut self, name: String, value: String) {
            self.env.insert(name, value);
        }
    }
}
