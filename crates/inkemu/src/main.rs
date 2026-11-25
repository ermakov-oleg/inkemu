fn main() {
    let args = std::env::args_os().skip(1).collect();
    std::process::exit(inkemu::cli::main(args));
}
