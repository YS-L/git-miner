use clap::Clap;
use git2::Repository;
use git2::ObjectType;
use git2::Oid;
use git2::Commit;
use git2::Buf;
use std::time::SystemTime;
use std::sync::mpsc::{channel, Sender, TryRecvError};
use std::thread;

struct HashPrefixChecker {
    bytes: Vec<u8>,
    is_odd_length: bool,
}

impl HashPrefixChecker {

    fn new(prefix: &str) -> HashPrefixChecker {
        if prefix == "" {
            panic!("Prefix is empty");
        }
        if prefix.len() > 40 {
            panic!("Prefix is longer than 40 characters")
        }
        let is_odd_length = prefix.len() % 2 == 1;
        let mut _s = prefix.to_owned();
        if is_odd_length {
            _s.push_str("0");
        }
        let bytes = hex::decode(_s.as_str()).unwrap();
        HashPrefixChecker { bytes, is_odd_length }
    }

    fn check_prefix(&self, bytes: &[u8]) -> bool {
        for i in 0..self.bytes.len() - 1 {
            if self.bytes.get(i).unwrap() != bytes.get(i).unwrap() {
                return false
            }
        }
        let last_expected = *(self.bytes.last().unwrap());
        let last = *(bytes.get(self.bytes.len() - 1).unwrap());
        if self.is_odd_length {
            return last_expected == (last & 0b11110000)
        }
        last_expected == last
    }

}

fn mine_hash(tid: i64, tx: &Sender<(i64, Oid, String)>, prefix: String) {

    let repo = Repository::discover(".").unwrap();
    let head = repo.head().unwrap();
    let commit = head.peel_to_commit().unwrap();
    let commit_message = commit.message().unwrap();
    let tree = commit.tree().unwrap();
    let signature = repo.signature().unwrap();

    let mut i: i64 = 1;
    let checker = HashPrefixChecker::new(prefix.as_str());
    let parents: Vec<Commit> = commit.parents().collect();
    let parents_refs: Vec<&Commit> = parents.iter().collect();
    loop {
        let message = format!("{}\nNONCE {}:{}", commit_message, tid, i);
        let commit_buf = repo.commit_create_buffer(
            &signature,
            &signature,
            &message,
            &tree,
            &parents_refs,
        ).unwrap();
        let result_oid = Oid::hash_object(ObjectType::Commit, &commit_buf).unwrap();
        let hash_bytes = result_oid.as_bytes();
        if checker.check_prefix(&hash_bytes) {
            println!("thread {} found!!", tid);
            tx.send((i, result_oid, commit_buf.as_str().unwrap().to_owned())).unwrap();
        }
        i = i + 1;
    }
}

#[derive(Clap)]
#[clap(version="0.1.0", author="YS-L <liauys@gmail.com>")]
struct Opts {

    #[clap(short, long)]
    prefix: String,

    #[clap(long)]
    amend: bool,
}

fn main()  {
    let opts: Opts = Opts::parse();
    let prefix = opts.prefix;

    let repo = Repository::discover(".").unwrap();
    let mut head = repo.head().unwrap();
    let commit = head.peel_to_commit().unwrap();
    let now = SystemTime::now();

    let (tx, rx) = channel();

    let n_threads = 4;
    for i in 0..n_threads {
        let tx = tx.clone();
        let _prefix = prefix.clone();
        thread::spawn(move|| {
            mine_hash(i, &tx, _prefix);
        });
    }

    loop {
        match rx.try_recv() {
            Ok((i, result_oid, commit_buf_string)) => {
                let commit_buf = commit_buf_string.as_bytes();

                let elapsed = now.elapsed().unwrap();
                eprintln!("Found after {} tries!", i);
                eprintln!("Time taken: {} s", elapsed.as_secs_f64());
                eprintln!("Time per hash: {} us", 1000000.0 * elapsed.as_secs_f64() / (i as f64));
                println!("{}", result_oid);

                let odb = repo.odb().unwrap();
                odb.write(ObjectType::Commit, commit_buf).unwrap();

                if opts.amend {
                    eprintln!("Replacing the latest commit with {}", result_oid);
                    head.set_target(
                        result_oid,
                        format!("git-miner moved from {}", commit.id()).as_str(),
                    ).unwrap();
                }
                break;
            },
            Err(e) => {
                if let TryRecvError::Disconnected = e {
                    eprintln!("Thread exited");
                }
                continue;
            },
        }
    }
}

#[cfg(test)]
mod tests {

    use super::*;

    #[test]
    fn test_prefix_even() {
        let checker = HashPrefixChecker::new("1234");
        assert_eq!(checker.check_prefix(&vec![0x12, 0x34, 0x56]), true);
    }

    #[test]
    fn test_prefix_odd() {
        let checker = HashPrefixChecker::new("123");
        assert_eq!(checker.check_prefix(&vec![0x12, 0x30]), true);
        assert_eq!(checker.check_prefix(&vec![0x12, 0x39, 0x02]), true);
        assert_eq!(checker.check_prefix(&vec![0x12, 0x03, 0x03]), false);
    }

    #[test]
    fn test_prefix_length_one() {
        let checker = HashPrefixChecker::new("1");
        assert_eq!(checker.check_prefix(&vec![0x10]), true);
    }

    #[test]
    fn test_prefix_zeros() {
        let checker = HashPrefixChecker::new("000");
        assert_eq!(checker.check_prefix(&vec![0x00, 0x01]), true);
    }
}
