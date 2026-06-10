use proc_macro2::TokenStream;
use quote::quote;
use syn::{parse2, Data, DeriveInput, Error, Field, Lit};

pub fn derive(input: TokenStream) -> Result<TokenStream, Error> {
    let ast: DeriveInput = parse2(input)?;
    let name = &ast.ident;

    let fields = match &ast.data {
        Data::Struct(s) => &s.fields,
        _ => return Err(Error::new_spanned(&ast, "ConfigMeta only supports structs")),
    };

    let mut entries = Vec::new();

    for field in fields.iter() {
        let field_name = field
            .ident
            .as_ref()
            .ok_or_else(|| Error::new_spanned(field, "tuple structs not supported"))?;

        let meta = extract_config_meta_pub(field)?;
        let env = &meta.env;
        let cli = &meta.cli;
        let doc = &meta.doc;
        let since = &meta.since;
        let type_name = type_name_string(&field.ty);
        let name_str = field_name.to_string();

        entries.push(quote! {
            crate::config_meta_types::FieldMeta {
                name:          #name_str,
                env_var:       #env,
                cli_flag:      #cli,
                doc:           #doc,
                default_value: "",
                since:         #since,
                type_name:     #type_name,
            }
        });
    }

    Ok(quote! {
        impl #name {
            /// Returns static metadata for every configuration field.
            pub fn config_schema() -> &'static [crate::config_meta_types::FieldMeta] {
                static SCHEMA: &[crate::config_meta_types::FieldMeta] = &[ #(#entries),* ];
                SCHEMA
            }
        }
    })
}

/// Parsed attributes from `#[config_meta(...)]`.
pub struct ConfigMetaAttrs {
    pub env: String,
    pub cli: String,
    pub doc: String,
    pub since: String,
}

pub(crate) fn extract_config_meta_pub(field: &Field) -> Result<ConfigMetaAttrs, Error> {
    for attr in &field.attrs {
        if !attr.path().is_ident("config_meta") {
            continue;
        }
        let mut env = String::new();
        let mut cli = String::new();
        let mut doc = String::new();
        let mut since = String::new();

        attr.parse_nested_meta(|meta| {
            let key = meta
                .path
                .get_ident()
                .map(|i| i.to_string())
                .unwrap_or_default();
            let value: Lit = meta.value()?.parse()?;
            let s = match &value {
                Lit::Str(s) => s.value(),
                _ => return Err(meta.error("expected string literal")),
            };
            match key.as_str() {
                "env" => env = s,
                "cli" => cli = s,
                "doc" => doc = s,
                "since" => since = s,
                other => return Err(meta.error(format!("unknown key: {other}"))),
            }
            Ok(())
        })?;

        if env.is_empty() {
            return Err(Error::new_spanned(
                field,
                "config_meta requires `env = \"ANNO_...\"`",
            ));
        }
        if cli.is_empty() {
            return Err(Error::new_spanned(
                field,
                "config_meta requires `cli = \"--flag-name\"`",
            ));
        }
        if doc.is_empty() {
            return Err(Error::new_spanned(
                field,
                "config_meta requires `doc = \"description\"`",
            ));
        }
        return Ok(ConfigMetaAttrs {
            env,
            cli,
            doc,
            since,
        });
    }

    Err(Error::new_spanned(
        field,
        "field is missing `#[config_meta(env = \"...\", cli = \"...\", doc = \"...\", since = \"...\")]`",
    ))
}

fn type_name_string(ty: &syn::Type) -> String {
    quote!(#ty).to_string().replace(' ', "")
}
