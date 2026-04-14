fn main() {
    if let Err(err) = git_ai_commit::run(std::env::args().skip(1).collect()) {
        eprintln!("{err}");
        std::process::exit(1);
    }
}
