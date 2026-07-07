use emit_answer_macro::EmitAnswer;

#[derive(EmitAnswer)]
struct MacroAnswer;

fn main() {
    println!("{}", MacroAnswer::PROC_MACRO_MESSAGE);
}
