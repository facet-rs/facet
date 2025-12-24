#!/usr/bin/env python3
import re
import sys

def replace_tokio_main(content):
    # Pattern to match #[tokio::main]\nasync fn main() with optional return type
    pattern = r'#\[tokio::main\]\s*\nasync fn main\((.*?)\)(.*?)\{'

    def replacement(match):
        params = match.group(1)
        return_type = match.group(2).strip()

        if return_type:
            # Has return type
            new_main = f'''fn main({params}){return_type} {{
    let rt = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .unwrap();
    rt.block_on(async_main({params}))
}}

async fn async_main({params}){return_type} {{'''
        else:
            # No return type
            new_main = f'''fn main({params}) {{
    let rt = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .unwrap();
    rt.block_on(async_main({params}));
}}

async fn async_main({params}) {{'''

        return new_main

    return re.sub(pattern, replacement, content)

if __name__ == '__main__':
    filepath = sys.argv[1]
    with open(filepath, 'r') as f:
        content = f.read()

    new_content = replace_tokio_main(content)

    with open(filepath, 'w') as f:
        f.write(new_content)

    print(f"Updated: {filepath}")
