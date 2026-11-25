fn main() {
    let args: Vec<_> = std::env::args_os().skip(1).collect();
    // with arguments it is the `inkemu` CLI: emu_start spawns this executable as `serve ...`
    let code = if args.is_empty() {
        inkemu::mcp::main(None)
    } else {
        inkemu::cli::main(args)
    };
    std::process::exit(code);
}
