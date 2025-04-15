pub struct GenerateReadmeOpts {
    pub crate_name: String,
    pub input: String,
}

pub fn generate(opts: GenerateReadmeOpts) -> String {
    // Generate header by replacing "{CRATE}" in an included header template
    let header_template = include_str!("header.md");
    let header = header_template.replace("{CRATE}", &opts.crate_name);

    // The main template content, passed in via `opts.input`
    let template_content = opts.input;

    // Include footer template
    let footer = include_str!("footer.md").to_string();

    // Combine header, template, and footer with newlines
    format!("{}\n{}\n{}", header, template_content, footer)
}
