use std::env;
use std::path::Path;
pub struct ChangeDir {
    path: Box<Path>,
}
impl ChangeDir {
    pub fn eval(&self) -> i32 {
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
