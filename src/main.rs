use clap::Clap;
use git2::Commit;
use git2::ObjectType;
use git2::Oid;
use git2::Repository;
use git2::Signature;
use sha1::{Digest, Sha1};
use std::sync::mpsc::{channel, Sender};
use std::thread;
use std::time::{SystemTime, UNIX_EPOCH};

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
        HashPrefixChecker {
            bytes,
            is_odd_length,
        }
    }

    fn check_prefix(&self, bytes: &[u8]) -> bool {
        for i in 0..self.bytes.len() - 1 {
            if self.bytes.get(i).unwrap() != bytes.get(i).unwrap() {
                return false;
            }
        }
        let last_expected = *(self.bytes.last().unwrap());
        let last = *(bytes.get(self.bytes.len() - 1).unwrap());
        if self.is_odd_length {
            return last_expected == (last & 0b1111_0000);
        }
        last_expected == last
    }
}

enum Message {
    Progress(i64),
    Found((i64, Oid, String)),
}

fn format_signature_data(signature: &Signature) -> String {
    let mut data = String::from("");

    // TODO: test without name / email
    if let Some(name) = signature.name() {
        data += format!(" {}", name).as_str();
    }
    if let Some(email) = signature.email() {
        data += format!(" <{}>", email).as_str();
    }

    // TODO: handle -ve time zone
    let time = signature.when();
    data += format!(" {}", time.seconds()).as_str();
    data += format!(
        " +{:02}{:02}",
        time.offset_minutes() / 60,
        time.offset_minutes() % 60,
    )
    .as_str();

    data
}

fn mine_hash(
    tid: i64,
    tx: &Sender<Message>,
    prefix: String,
    repo_path: String,
    reset_author: bool,
) {
    let repo = Repository::discover(repo_path).unwrap();
    let head = repo.head().unwrap();
    let commit = head.peel_to_commit().unwrap();
    let commit_message = commit.message().unwrap();
    let tree = commit.tree().unwrap();

    let mut i: i64 = 1;
    let mut n_sum = 0;
    let checker = HashPrefixChecker::new(prefix.as_str());
    let parents: Vec<Commit> = commit.parents().collect();
    let parents_refs: Vec<&Commit> = parents.iter().collect();

    let mut author_signature = commit.author();
    let committer_signature = commit.committer();

    if reset_author {
        author_signature = repo.signature().unwrap();
    }

    let author_data = format_signature_data(&author_signature);
    let committer_data = format_signature_data(&committer_signature);

    // Form the parts of commit data that will not change
    let mut parts: Vec<String> = Vec::new();
    parts.push(format!("tree {}", tree.id()));
    if !parents.is_empty() {
        let parents_data = parents
            .iter()
            .map(|x| format!("parent {}", x.id()))
            .collect::<Vec<String>>()
            .join("\n");
        parts.push(parents_data);
    }
    parts.push(format!("author{}", author_data.as_str()));
    parts.push(format!("committer{}", committer_data.as_str()));
    parts.push(format!("\n{}", commit_message));
    let fixed_commit_data = parts.join("\n");

    // A bunch of unicode spaces :-)
    let chars = vec![
        [0x20, 0x20, 0x20], // U+0020
        [0xc2, 0xa0, 0x20], // U+00A0
        [0xe2, 0x80, 0x80], // U+2000
        [0xe2, 0x80, 0x81], // U+2001
        [0xe2, 0x80, 0x82], // U+2002
        [0xe2, 0x80, 0x83], // U+2003
        [0xe2, 0x80, 0x84], // ...
        [0xe2, 0x80, 0x85],
        [0xe2, 0x80, 0x86],
        [0xe2, 0x80, 0x87],
        [0xe2, 0x80, 0x88],
        [0xe2, 0x80, 0x89],
        [0xe2, 0x80, 0x8a], // U+200A
        [0xe2, 0x80, 0x8b], // U+200B
        [0xe2, 0x80, 0xaf], // U+202F
        [0xe2, 0x81, 0x9f], // U+205F
    ];
    let nonce_len = 3 * 20;
    let mut nonce_bytes = vec![0x20; nonce_len];

    let all_except_nonce = format!(
        "commit {}\0{}",
        fixed_commit_data.len() + nonce_len,
        fixed_commit_data,
    );

    let mut sh = Sha1::default();
    sh.update(all_except_nonce.as_bytes());

    loop {
        n_sum += 1;

        let mut _sh = sh.clone();
        _sh.update(&nonce_bytes);
        if i == 1 {
            // Inject something thread specific so that each thread can explore a different path.
            // But this also means that the first hash will not be valid.
            _sh.update(format!("{}", tid).as_bytes());
        }
        let res_bytes = _sh.finalize();

        if i > 1 && checker.check_prefix(&res_bytes) {
            let nonce = String::from_utf8(nonce_bytes).unwrap();
            let message = format!("{}{}", commit_message, nonce.as_str());
            let commit_buf = repo
                .commit_create_buffer(
                    &author_signature,
                    &committer_signature,
                    &message,
                    &tree,
                    &parents_refs,
                )
                .unwrap();

            // verify sha1 is done correctly
            let res_oid = Oid::from_bytes(&res_bytes).unwrap();
            let git_oid = Oid::hash_object(ObjectType::Commit, &commit_buf).unwrap();
            let git_bytes = git_oid.as_bytes();
            if git_bytes != &res_bytes[..] {
                panic!("Commit's hash is not the same as the SHA1 hash!")
            }

            let buf = commit_buf.as_str().unwrap().to_owned();
            let m = Message::Found((n_sum, res_oid, buf));
            tx.send(m).unwrap();
            break;
        } else {
            // Use the current sha to determine what the next nonce bytes are
            for (i, byte) in res_bytes.iter().enumerate() {
                let j = (byte & 0x0f) as usize;
                nonce_bytes[i * 3..i * 3 + 3].copy_from_slice(&chars[j][..]);
            }
        }
        i += 1;
        if n_sum >= 10000 {
            tx.send(Message::Progress(n_sum)).unwrap();
            n_sum = 0;
        }
    }
}

#[derive(Clap)]
#[clap(version = "0.1.0", author = "YS-L <liauys@gmail.com>")]
struct Opts {
    #[clap(short, long)]
    prefix: String,

    #[clap(long)]
    amend: bool,

    #[clap(long)]
    reset_author: bool,

    #[clap(long, default_value = "1")]
    threads: String,

    #[clap(long, default_value = ".")]
    repo: String,
}

fn get_time_since_epoch() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_millis()
}

fn main() {
    let opts: Opts = Opts::parse();
    let prefix = opts.prefix;
    let repo_path = opts.repo;
    let reset_author = opts.reset_author;

    let repo = Repository::discover(repo_path.as_str()).unwrap();
    let mut head = repo.head().unwrap();
    let commit = head.peel_to_commit().unwrap();
    let now = SystemTime::now();

    let (tx, rx) = channel();

    let n_threads = opts.threads.parse::<i64>().unwrap();
    eprintln!("Using {} threads", n_threads);
    for i in 0..n_threads {
        let tx = tx.clone();
        let _prefix = prefix.clone();
        let _repo_path = repo_path.clone();
        thread::spawn(move || {
            mine_hash(i, &tx, _prefix, _repo_path, reset_author);
        });
    }

    let mut n_hashed: i64 = 0;
    let mut time_last_reported = get_time_since_epoch();
    let mut prev_progress_len = 0;

    for m in rx.iter() {
        match m {
            Message::Found((i, result_oid, commit_buf_string)) => {
                let commit_buf = commit_buf_string.as_bytes();

                let elapsed = now.elapsed().unwrap();
                n_hashed += i;
                let time_per_hash = elapsed.as_secs_f64() / (n_hashed as f64);
                eprintln!("\nFound after {} tries!", n_hashed);
                eprintln!("Time taken: {:.2} s", elapsed.as_secs_f64());
                eprintln!(
                    "Average time per hash: {:.3} us",
                    1_000_000.0 * time_per_hash
                );

                println!("{}", result_oid);

                let odb = repo.odb().unwrap();
                odb.write(ObjectType::Commit, commit_buf).unwrap();

                if opts.amend {
                    eprintln!("Replacing the latest commit with {}", result_oid);
                    head.set_target(
                        result_oid,
                        format!("git-miner moved from {}", commit.id()).as_str(),
                    )
                    .unwrap();
                }
                break;
            }
            Message::Progress(i) => {
                n_hashed += i;
                let cur = get_time_since_epoch();
                if (cur - time_last_reported) > 100 {
                    let elapsed = now.elapsed().unwrap();
                    let rate = 1_000_000.0 * elapsed.as_secs_f64() / (n_hashed as f64);
                    let progress = format!(
                        "Computed {} hashes. Effective rate = {:.3} us per hash",
                        n_hashed, rate,
                    );
                    eprint!("\r{}", " ".repeat(prev_progress_len));
                    eprint!("\r{}", progress);
                    prev_progress_len = progress.len();
                    time_last_reported = cur;
                }
            }
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
