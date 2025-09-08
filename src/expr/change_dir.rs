use std::env;
use std::path::Path;

fn normalize_path(path: &str) -> String {
    // this feels a bit untidy if im being honest... far too much state
    let mut path_stack: Vec<String> = Vec::new();
    let mut current = String::new();
    let mut prev: char = '\0';
    for c in path.chars() {
        if c != '/' || prev == '\\' {
            current.push(c);
        } else {
            if current == ".." {
                path_stack.pop();
            } else if !current.is_empty() {
                path_stack.push(current.clone());
            }
            current.clear();
        }
        prev = c;
    }
    if current == ".." {
        path_stack.pop();
    } else if !current.is_empty() {
        path_stack.push(current.clone());
    }
    String::from("/") + &path_stack.join("/")
}

pub struct ChangeDir {
    path: Box<Path>,
}
impl ChangeDir {
    pub fn eval(&self) -> i32 {
        unsafe {
            env::set_var(
                "PWD",
                normalize_path(&String::from(
                    Path::new(&env::var("PWD").unwrap_or("".to_string()))
                        .join(&self.path)
                        .as_os_str()
                        .to_str()
                        .unwrap_or(""),
                )),
            );
        }
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

mod tests {
    #[allow(unused_imports)]
    use crate::expr::change_dir::normalize_path;
    #[test]
    fn test_normalize_paths() {
        let path = normalize_path(&String::from("/patha/../pathb"));
        println!("Got path: {}", path);
        assert!(path == "/pathb");

        let path = normalize_path(&String::from("/patha//../pathb"));
        println!("Got path: {}", path);
        assert!(path == "/pathb");

        let path = normalize_path(&String::from("/patha/poop//../pathb"));
        println!("Got path: {}", path);
        assert!(path == "/patha/pathb");

        let path = normalize_path(&String::from("/patha/space path//../pathb"));
        println!("Got path: {}", path);
        assert!(path == "/patha/pathb");

        let path = normalize_path(&String::from("/patha/space\\/path//../pathb"));
        println!("Got path: {}", path);
        assert!(path == "/patha/pathb");
    }
}
