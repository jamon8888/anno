use proc_macro2::TokenStream;
use quote::quote;
use syn::{Data, DeriveInput, Error, GenericArgument, PathArguments, Type, parse2};

use crate::config_meta::extract_config_meta_pub;

pub fn derive(input: TokenStream) -> Result<TokenStream, Error> {
    let ast: DeriveInput = parse2(input)?;

    let fields = match &ast.data {
        Data::Struct(s) => &s.fields,
        _ => return Err(Error::new_spanned(&ast, "ConfigCliArgs only supports structs")),
    };

    let mut field_tokens = Vec::new();

    for field in fields.iter() {
        let field_name = field
            .ident
            .as_ref()
            .ok_or_else(|| Error::new_spanned(field, "tuple structs not supported"))?;

        let meta = extract_config_meta_pub(field)?;
        let env_str = &meta.env;
        let cli_str = meta.cli.trim_start_matches('-').to_string();
        let doc_str = &meta.doc;

        let inner_ty = inner_option_type(&field.ty);
        let field_ty = match inner_ty {
            Some(t) => quote! { Option<#t> },
            None => {
                let ty = &field.ty;
                quote! { Option<#ty> }
            }
        };

        field_tokens.push(quote! {
            #[arg(long = #cli_str, env = #env_str, help = #doc_str)]
            pub #field_name: #field_ty
        });
    }

    Ok(quote! {
        #[derive(clap::Args, Debug, Default, Clone)]
        pub struct ConfigOverrides {
            #(#field_tokens),*
        }
    })
}

fn inner_option_type(ty: &Type) -> Option<&Type> {
    if let Type::Path(tp) = ty {
        let seg = tp.path.segments.last()?;
        if seg.ident != "Option" {
            return None;
        }
        if let PathArguments::AngleBracketed(ab) = &seg.arguments {
            if let Some(GenericArgument::Type(t)) = ab.args.first() {
                return Some(t);
            }
        }
    }
    None
}
