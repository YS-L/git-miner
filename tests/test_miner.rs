use std::env;
use std::fs::File;
use std::io::Write;
use std::process::{Command, Output};
use std::str;
use std::path::PathBuf;
use tempfile::TempDir;


struct GitRepo {
    dir: PathBuf,
}

impl GitRepo {

    pub fn new(repo_dir: PathBuf) -> GitRepo {
        let repo = GitRepo { dir: repo_dir };
        repo.git(&["init"]);
        return repo;
    }

    pub fn add_and_commit(&self, filename: &str, content: &str) {
        let file_path = self.dir.join(filename);
        let mut file = File::create(&file_path).unwrap();
        write!(file, "{}", content).unwrap();
        let file_name = file_path.file_name().unwrap().to_str().unwrap();
        self.git(&["add", file_name]);
        self.commit(format!("Added {}", file_name).as_str());
    }

    pub fn commit(&self, message: &str) {
        self.git(&["commit", "-m", message]);
    }

    pub fn status(&self) {
        println!("{}", &self.git_output(&["status"]));
    }

    pub fn latest_commit_sha(&self) -> String {
        self.git_output(&["log", "-1", "--format=format:%H"])
    }

    fn git(&self, args: &[&str]) -> Output {
        println!("---- Running git command: {:?} ----", args);
        let res = Command::new("git")
                          .current_dir(&self.dir)
                          .args(args)
                          .output()
                          .expect("git error");
        println!("\tgit stdout:\n{}", str::from_utf8(res.stdout.as_slice()).unwrap());
        println!("\tgit stderr:\n{}", str::from_utf8(res.stderr.as_slice()).unwrap());
        return res;
    }

    fn git_output(&self, args: &[&str]) -> String {
        let output = self.git(args);
        return format!("{}", str::from_utf8(output.stdout.as_slice()).unwrap());
    }

}


#[test]
fn simple_prefix() {

    let temp_dir = TempDir::new().unwrap();
    let repo = GitRepo::new(temp_dir.path().to_path_buf());
    repo.add_and_commit("a.txt", "Something A");
    repo.add_and_commit("b.txt", "Something B");
    repo.add_and_commit("c.txt", "Something C");

    let mut root = env::current_exe()
                       .unwrap()
                       .parent()
                       .expect("failed to get exe directory")
                       .to_path_buf();
    if root.ends_with("deps") {
        root.pop();
    }
    let git_miner_bin = root.join("git-miner");

    let _res = Command::new(git_miner_bin)
                       .current_dir(&temp_dir.path())
                       .args(&["--prefix", "000", "--amend"])
                       .output()
                       .unwrap();

    let latest_commit_sha = repo.latest_commit_sha();

    assert!(&latest_commit_sha[..3] == "000");

}
