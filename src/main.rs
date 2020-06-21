use git2::Repository;
use git2::ObjectType;
use git2::Oid;
use std::time::SystemTime;


fn main()  {
    let repo = Repository::discover("/home/liauys/Code/test-repo").unwrap();
    let head = repo.head().unwrap();
    let commit = head.peel_to_commit().unwrap();
    let tree = commit.tree().unwrap();
    let signature = repo.signature().unwrap();
    let mut i: i64 = 1;
    let now = SystemTime::now();
    loop {
        let commit_buf = repo.commit_create_buffer(
            &signature,
            &signature,
            &format!("Test creating a commit that starts with 0\n\nNONCE {}", i),
            &tree,
            &[&commit],
        ).unwrap();
        let result_oid = Oid::hash_object(ObjectType::Commit, &commit_buf).unwrap();
        let hash_bytes = result_oid.as_bytes();
        if hash_bytes[0] == 0 {
            let elapsed = now.elapsed().unwrap();
            println!("Found after {} tries! {}", i, result_oid);
            println!("Time taken: {} s", elapsed.as_secs_f64());
            println!("Time per hash: {} us", 1000000.0 * elapsed.as_secs_f64() / (i as f64));
            let odb = repo.odb().unwrap();
            odb.write(ObjectType::Commit, &commit_buf).unwrap();
            break;
        }
        i = i + 1;
    }
}
