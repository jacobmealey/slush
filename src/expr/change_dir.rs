use clean_path::Clean; // crate for 'cleaning' paths, turns some/path//../other_path into
                       // some/other_path, used for setting PWD
use std::env;
use std::path::Path;

pub struct ChangeDir {
    path: Box<Path>,
}
impl ChangeDir {
    pub fn eval(&self) -> i32 {
        env::set_var(
            "PWD",
            Path::new(&env::var("PWD").unwrap_or("".to_string()))
                .join(&self.path)
                .clean()
                .as_os_str(),
        );
        match env::set_current_dir(&self.path) {
            Ok(()) => 0,
            Err(_) => 1,
        }
    }

    pub fn new(path: &str) -> ChangeDir {
        ChangeDir {
            path: Path::new(path).into(),
        }
    }
}
