use proc_macro::TokenStream;
use quote::quote;
use std::fs;

#[proc_macro]
pub fn load(_item: proc_macro::TokenStream) -> proc_macro::TokenStream {
    let mut components = Vec::new();

    if let Ok(entries) = fs::read_dir("./components") {
        for entry in entries {
            if let Ok(entry) = entry {
                let path = entry.path();
                if path.is_file() {
                    if let Some(name) = path.file_name() {
                        if let Some(name) = name.to_str() {
                            println!("{}", name);
                            if name.ends_with(".jsx") {
                                if let Ok(content) = fs::read_to_string(&path) {
                                    components.push((name.to_string(), content));
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    let component_array = components.iter().map(|(name, content)| {
        quote! {
            (#name, #content)
        }
    });

    let expanded = quote! {
        &[#(#component_array),*]
    };

    TokenStream::from(expanded)
}
