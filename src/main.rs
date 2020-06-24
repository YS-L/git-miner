use clap::Clap;
use git2::Repository;
use git2::ObjectType;
use git2::Oid;
use git2::Commit;
use std::time::{SystemTime, UNIX_EPOCH};
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

enum Message {
    Progress(i64),
    Found((i64, Oid, String)),
}

fn mine_hash(tid: i64, tx: &Sender<Message>, prefix: String) {

    let repo = Repository::discover(".").unwrap();
    let head = repo.head().unwrap();
    let commit = head.peel_to_commit().unwrap();
    let commit_message = commit.message().unwrap();
    let tree = commit.tree().unwrap();
    let signature = repo.signature().unwrap();

    let mut i: i64 = 1;
    let mut n_sum = 0;
    let checker = HashPrefixChecker::new(prefix.as_str());
    let parents: Vec<Commit> = commit.parents().collect();
    let parents_refs: Vec<&Commit> = parents.iter().collect();
    loop {
        n_sum = n_sum + 1;
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
            let buf = commit_buf.as_str().unwrap().to_owned();
            let m = Message::Found((n_sum, result_oid, buf));
            tx.send(m).unwrap();
            break;
        }
        i = i + 1;
        if n_sum >= 10000 {
            tx.send(Message::Progress(n_sum)).unwrap();
            n_sum = 0;
        }
    }
}

#[derive(Clap)]
#[clap(version="0.1.0", author="YS-L <liauys@gmail.com>")]
struct Opts {

    #[clap(short, long)]
    prefix: String,

    #[clap(long)]
    amend: bool,

    #[clap(long, default_value="1")]
    threads: String,
}

fn get_time_since_epoch() -> u128 {
    return SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_millis();
}

fn main()  {
    let opts: Opts = Opts::parse();
    let prefix = opts.prefix;

    let repo = Repository::discover(".").unwrap();
    let mut head = repo.head().unwrap();
    let commit = head.peel_to_commit().unwrap();
    let now = SystemTime::now();

    let (tx, rx) = channel();


    let n_threads = opts.threads.parse::<i64>().unwrap();
    eprintln!("Using {} threads", n_threads);
    for i in 0..n_threads {
        let tx = tx.clone();
        let _prefix = prefix.clone();
        thread::spawn(move|| {
            mine_hash(i, &tx, _prefix);
        });
    }

    let mut n_hashed: i64 = 0;
    let mut time_last_reported = get_time_since_epoch();
    let mut prev_progress_len = 0;

    loop {
        match rx.try_recv() {
            Ok(Message::Found((i, result_oid, commit_buf_string))) => {
                let commit_buf = commit_buf_string.as_bytes();

                let elapsed = now.elapsed().unwrap();
                n_hashed += i;
                let time_per_hash = elapsed.as_secs_f64() / (n_hashed as f64);
                eprintln!("\nFound after {} tries!", n_hashed);
                eprintln!("Time taken: {:.2} s", elapsed.as_secs_f64());
                eprintln!("Average time per hash: {:.2} us", 1000000.0 * time_per_hash);

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
            Ok(Message::Progress(i)) => {
                n_hashed += i;
                let cur = get_time_since_epoch();
                if (cur - time_last_reported) > 100 {
                    let elapsed = now.elapsed().unwrap();
                    let rate = 1000000.0 * elapsed.as_secs_f64() / (n_hashed as f64);
                    let progress = format!(
                        "Computed {} hashes. Effective rate = {:.2} us per hash",
                        n_hashed,
                        rate,
                    );
                    eprint!("\r{}", " ".repeat(prev_progress_len));
                    eprint!("\r{}", progress);
                    prev_progress_len = progress.len();
                    time_last_reported = cur;
                }
            }
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
