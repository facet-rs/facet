use std::io::Read;
use styx_parse::Lexer;
use styx_tokenizer::Tokenizer;

fn main() {
    let mut source = String::new();
    std::io::stdin().read_to_string(&mut source).unwrap();

    println!("=== Tokens ===");
    for tok in Tokenizer::new(&source) {
        println!("{:?}", tok);
    }

    println!("\n=== Lexemes ===");
    for lex in Lexer::new(&source) {
        println!("{:?}", lex);
    }
}
