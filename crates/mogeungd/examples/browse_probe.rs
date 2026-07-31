fn main() {
    match mogeungd::discovery::browse(std::time::Duration::from_secs(3)) {
        Ok(v) if v.is_empty() => println!("FOUND NOTHING"),
        Ok(v) => for f in v {
            println!("FOUND {} at {} needs_token={} home={:?}", f.name, f.addr, f.needs_token, f.claude_home);
        },
        Err(e) => println!("ERROR {e}"),
    }
}
