use proc_macro::TokenStream;
use quote::quote;
use std::fs;

#[proc_macro]
pub fn load(_item: proc_macro::TokenStream) -> proc_macro::TokenStream {
    let mut components = Vec::new();

    if let Ok(entries) = fs::read_dir("components") {
        for entry in entries {
            if let Ok(path) = entry.map(|entry| entry.path()) {
                if path.is_file() && path.extension().is_some_and(|ext| ext == "jsx") {
                    if let Some(name) = path.file_name().and_then(|name| name.to_str()) {
                        if let Ok(content) = fs::read_to_string(&path) {
                            components.push((name.to_string(), content));
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
