//! # anno-derive
//!
//! Procedural derive macros for the anno NLP toolkit.
//!
//! ## `#[derive(Dataset)]`
//!
//! Automatically implements dataset metadata accessors, `FromStr`, and `Display`
//! for dataset enum types.
//!
//! ### Example
//!
//! ```ignore
//! use anno_derive::Dataset;
//!
//! #[derive(Dataset, Debug, Clone, PartialEq)]
//! pub enum DatasetId {
//!     /// CoNLL 2003 English NER dataset
//!     #[dataset(
//!         name = "CoNLL-2003",
//!         task = "ner",
//!         languages("en"),
//!         entity_types("PER", "LOC", "ORG", "MISC"),
//!         url = "https://www.clips.uantwerpen.be/conll2003/ner/"
//!     )]
//!     Conll2003,
//!     
//!     /// OntoNotes 5.0 dataset
//!     #[dataset(
//!         name = "OntoNotes 5.0",
//!         task = "ner",
//!         languages("en", "zh", "ar"),
//!         entity_types("PERSON", "ORG", "GPE", "LOC", "FAC", "NORP", "PRODUCT",
//!                        "EVENT", "WORK_OF_ART", "LAW", "LANGUAGE", "DATE", "TIME",
//!                        "PERCENT", "MONEY", "QUANTITY", "ORDINAL", "CARDINAL"),
//!         aliases("ontonotes", "onto")
//!     )]
//!     OntoNotes5,
//! }
//! ```
//!
//! This generates the following inherent methods:
//! - `fn name(&self) -> &'static str` - Human-readable name
//! - `fn task(&self) -> &'static str` - Task type (e.g., "ner", "coref")
//! - `fn languages(&self) -> &'static [&'static str]` - Supported languages
//! - `fn entity_types(&self) -> &'static [&'static str]` - Entity types
//! - `fn url(&self) -> Option<&'static str>` - Dataset URL
//! - `fn description(&self) -> Option<&'static str>` - Dataset description
//! - `fn source(&self) -> Option<&'static str>` - Data source
//! - `fn all() -> [Self; N]` - All variants
//! - `fn count() -> usize` - Number of variants
//! - `fn is_multilingual(&self) -> bool` - Has multiple languages
//! - `fn supports_task(&self, task: &str) -> bool` - Task match check
//! - `fn has_entity_type(&self, entity_type: &str) -> bool` - Entity type check
//! - `fn supports_language(&self, lang: &str) -> bool` - Language check
//! - `fn by_task(task: &str) -> Vec<Self>` - Filter by task
//! - `fn by_language(lang: &str) -> Vec<Self>` - Filter by language
//!
//! And these trait implementations:
//! - `impl FromStr for DatasetId` - Parse from string (with aliases)
//! - `impl Display for DatasetId` - Format as display name

use proc_macro::TokenStream;
use proc_macro2::{Ident, TokenStream as TokenStream2};
use quote::quote;
use syn::{
    parse_macro_input, punctuated::Punctuated, spanned::Spanned, Attribute, Data, DeriveInput,
    Fields, Token,
};

/// Derive macro for dataset enums.
///
/// Generates metadata accessors, `FromStr`, and `Display` implementations.
#[proc_macro_derive(Dataset, attributes(dataset))]
pub fn derive_dataset(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);

    match impl_dataset(&input) {
        Ok(tokens) => tokens.into(),
        Err(err) => err.to_compile_error().into(),
    }
}

/// Parsed dataset attributes from `#[dataset(...)]`
#[derive(Default, Debug)]
struct DatasetAttrs {
    name: Option<String>,
    task: Option<String>,
    languages: Vec<String>,
    entity_types: Vec<String>,
    url: Option<String>,
    description: Option<String>,
    aliases: Vec<String>,
    source: Option<String>,
}

impl DatasetAttrs {
    fn parse_from_attrs(attrs: &[Attribute]) -> syn::Result<Self> {
        let mut result = Self::default();

        for attr in attrs {
            if !attr.path().is_ident("dataset") {
                continue;
            }

            attr.parse_nested_meta(|meta| {
                let ident = meta
                    .path
                    .get_ident()
                    .ok_or_else(|| syn::Error::new(meta.path.span(), "expected identifier"))?;

                match ident.to_string().as_str() {
                    "name" => {
                        let value: syn::LitStr = meta.value()?.parse()?;
                        result.name = Some(value.value());
                    }
                    "task" => {
                        let value: syn::LitStr = meta.value()?.parse()?;
                        result.task = Some(value.value());
                    }
                    "url" => {
                        let value: syn::LitStr = meta.value()?.parse()?;
                        result.url = Some(value.value());
                    }
                    "description" => {
                        let value: syn::LitStr = meta.value()?.parse()?;
                        result.description = Some(value.value());
                    }
                    "source" => {
                        let value: syn::LitStr = meta.value()?.parse()?;
                        result.source = Some(value.value());
                    }
                    "languages" => {
                        let content;
                        syn::parenthesized!(content in meta.input);
                        let strings: Punctuated<syn::LitStr, Token![,]> =
                            Punctuated::parse_terminated(&content)?;
                        result.languages = strings.iter().map(|s| s.value()).collect();
                    }
                    "entity_types" => {
                        let content;
                        syn::parenthesized!(content in meta.input);
                        let strings: Punctuated<syn::LitStr, Token![,]> =
                            Punctuated::parse_terminated(&content)?;
                        result.entity_types = strings.iter().map(|s| s.value()).collect();
                    }
                    "aliases" => {
                        let content;
                        syn::parenthesized!(content in meta.input);
                        let strings: Punctuated<syn::LitStr, Token![,]> =
                            Punctuated::parse_terminated(&content)?;
                        result.aliases = strings.iter().map(|s| s.value()).collect();
                    }
                    _ => {
                        return Err(syn::Error::new(
                            ident.span(),
                            format!("unknown dataset attribute: {}", ident),
                        ));
                    }
                }
                Ok(())
            })?;
        }

        Ok(result)
    }
}

/// Main implementation
fn impl_dataset(input: &DeriveInput) -> syn::Result<TokenStream2> {
    let name = &input.ident;

    let variants = match &input.data {
        Data::Enum(data) => &data.variants,
        _ => {
            return Err(syn::Error::new(
                input.span(),
                "Dataset can only be derived for enums",
            ));
        }
    };

    // Collect variant info
    let mut variant_data: Vec<(&Ident, DatasetAttrs)> = Vec::new();

    for variant in variants {
        // Only unit variants supported
        if !matches!(&variant.fields, Fields::Unit) {
            return Err(syn::Error::new(
                variant.span(),
                "Dataset only supports unit variants",
            ));
        }

        let attrs = DatasetAttrs::parse_from_attrs(&variant.attrs)?;
        variant_data.push((&variant.ident, attrs));
    }

    // Generate name() method
    let name_arms = variant_data.iter().map(|(ident, attrs)| {
        let name_str = attrs.name.clone().unwrap_or_else(|| ident.to_string());
        quote! {
            Self::#ident => #name_str,
        }
    });

    // Generate task() method
    let task_arms = variant_data.iter().map(|(ident, attrs)| {
        let task_str = attrs.task.as_deref().unwrap_or("ner");
        quote! {
            Self::#ident => #task_str,
        }
    });

    // Generate languages() method
    let languages_arms = variant_data.iter().map(|(ident, attrs)| {
        let langs = &attrs.languages;
        if langs.is_empty() {
            quote! {
                Self::#ident => &["en"],
            }
        } else {
            quote! {
                Self::#ident => &[#(#langs),*],
            }
        }
    });

    // Generate entity_types() method
    let entity_types_arms = variant_data.iter().map(|(ident, attrs)| {
        let types = &attrs.entity_types;
        if types.is_empty() {
            quote! {
                Self::#ident => &[],
            }
        } else {
            quote! {
                Self::#ident => &[#(#types),*],
            }
        }
    });

    // Generate url() method
    let url_arms = variant_data.iter().map(|(ident, attrs)| match &attrs.url {
        Some(url) => quote! {
            Self::#ident => Some(#url),
        },
        None => quote! {
            Self::#ident => None,
        },
    });

    // Generate description() method
    let description_arms = variant_data
        .iter()
        .map(|(ident, attrs)| match &attrs.description {
            Some(desc) => quote! {
                Self::#ident => Some(#desc),
            },
            None => quote! {
                Self::#ident => None,
            },
        });

    // Generate source() method
    let source_arms = variant_data
        .iter()
        .map(|(ident, attrs)| match &attrs.source {
            Some(src) => quote! {
                Self::#ident => Some(#src),
            },
            None => quote! {
                Self::#ident => None,
            },
        });

    // Generate FromStr - collect all aliases
    let from_str_arms: Vec<_> = variant_data
        .iter()
        .flat_map(|(ident, attrs)| {
            let ident_lower = ident.to_string().to_lowercase();
            let ident_snake = to_snake_case(&ident.to_string());
            let ident_kebab = ident_snake.replace('_', "-");

            let mut patterns = vec![ident_lower.clone(), ident_snake, ident_kebab];

            // Add explicit aliases
            for alias in &attrs.aliases {
                patterns.push(alias.to_lowercase());
            }

            // Add name as alias if different
            if let Some(name) = &attrs.name {
                let name_lower = name.to_lowercase();
                if !patterns.contains(&name_lower) {
                    patterns.push(name_lower);
                }
            }

            // Deduplicate
            patterns.sort();
            patterns.dedup();

            patterns
                .into_iter()
                .map(move |pattern| {
                    quote! {
                        #pattern => Ok(Self::#ident),
                    }
                })
                .collect::<Vec<_>>()
        })
        .collect();

    // Generate Display - use name
    let display_arms = variant_data.iter().map(|(ident, attrs)| {
        let display_name = attrs
            .name
            .as_deref()
            .map(|s| s.to_string())
            .unwrap_or_else(|| ident.to_string());
        quote! {
            Self::#ident => write!(f, #display_name),
        }
    });

    // Generate all() method
    let all_variants: Vec<_> = variant_data
        .iter()
        .map(|(ident, _)| {
            quote! { Self::#ident }
        })
        .collect();
    let all_count = all_variants.len();

    let expanded = quote! {
        impl #name {
            /// Returns the human-readable name of this dataset.
            #[must_use]
            pub fn name(&self) -> &'static str {
                match self {
                    #(#name_arms)*
                }
            }

            /// Returns the task type (e.g., "ner", "coref", "re").
            #[must_use]
            pub fn task(&self) -> &'static str {
                match self {
                    #(#task_arms)*
                }
            }

            /// Returns the languages this dataset supports.
            #[must_use]
            pub fn languages(&self) -> &'static [&'static str] {
                match self {
                    #(#languages_arms)*
                }
            }

            /// Returns the entity types in this dataset.
            #[must_use]
            pub fn entity_types(&self) -> &'static [&'static str] {
                match self {
                    #(#entity_types_arms)*
                }
            }

            /// Returns the URL for this dataset, if known.
            #[must_use]
            pub fn url(&self) -> Option<&'static str> {
                match self {
                    #(#url_arms)*
                }
            }

            /// Returns the description of this dataset, if available.
            #[must_use]
            pub fn description(&self) -> Option<&'static str> {
                match self {
                    #(#description_arms)*
                }
            }

            /// Returns the source of this dataset (e.g., "HuggingFace", "LDC").
            #[must_use]
            pub fn source(&self) -> Option<&'static str> {
                match self {
                    #(#source_arms)*
                }
            }

            /// Returns all dataset variants.
            #[must_use]
            pub fn all() -> [Self; #all_count] {
                [#(#all_variants),*]
            }

            /// Returns the number of datasets.
            #[must_use]
            pub const fn count() -> usize {
                #all_count
            }

            /// Returns true if this dataset supports multiple languages.
            #[must_use]
            pub fn is_multilingual(&self) -> bool {
                self.languages().len() > 1
            }

            /// Returns true if this dataset supports the given task.
            #[must_use]
            pub fn supports_task(&self, task: &str) -> bool {
                self.task().eq_ignore_ascii_case(task)
            }

            /// Returns true if this dataset has the given entity type.
            #[must_use]
            pub fn has_entity_type(&self, entity_type: &str) -> bool {
                self.entity_types().iter().any(|t| t.eq_ignore_ascii_case(entity_type))
            }

            /// Returns true if this dataset supports the given language.
            #[must_use]
            pub fn supports_language(&self, lang: &str) -> bool {
                self.languages().iter().any(|l| l.eq_ignore_ascii_case(lang))
            }

            /// Filter datasets by task.
            #[must_use]
            pub fn by_task(task: &str) -> ::std::vec::Vec<Self> {
                Self::all().into_iter().filter(|d| d.supports_task(task)).collect()
            }

            /// Filter datasets by language.
            #[must_use]
            pub fn by_language(lang: &str) -> ::std::vec::Vec<Self> {
                Self::all().into_iter().filter(|d| d.supports_language(lang)).collect()
            }
        }

        impl ::core::str::FromStr for #name {
            type Err = String;

            fn from_str(s: &str) -> Result<Self, Self::Err> {
                match s.to_lowercase().as_str() {
                    #(#from_str_arms)*
                    _ => Err(format!("unknown dataset: {}", s)),
                }
            }
        }

        impl ::core::fmt::Display for #name {
            fn fmt(&self, f: &mut ::core::fmt::Formatter<'_>) -> ::core::fmt::Result {
                match self {
                    #(#display_arms)*
                }
            }
        }
    };

    Ok(expanded)
}

/// Convert CamelCase to snake_case.
///
/// Inserts underscores only at lowercase→uppercase transitions.
///
/// Examples:
/// - "OntoNotes5" -> "onto_notes5"
/// - "WikiANN" -> "wiki_ann"
/// - "CoNLL2003" -> "co_nll2003"
/// - "ACE2005" -> "ace2005"
fn to_snake_case(s: &str) -> String {
    let mut result = String::new();
    let chars: Vec<char> = s.chars().collect();

    for (i, &c) in chars.iter().enumerate() {
        if c.is_uppercase() {
            // Insert underscore only when transitioning from lowercase to uppercase
            // This keeps consecutive uppercase letters together
            if i > 0 && chars[i - 1].is_lowercase() {
                result.push('_');
            }
            result.push(c.to_ascii_lowercase());
        } else {
            result.push(c);
        }
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_snake_case() {
        // Standard camel case
        assert_eq!(to_snake_case("OntoNotes5"), "onto_notes5");
        assert_eq!(to_snake_case("Simple"), "simple");
        // Trailing uppercase acronym
        assert_eq!(to_snake_case("WikiANN"), "wiki_ann");
        assert_eq!(to_snake_case("MasakhaNER"), "masakha_ner");
        // Mixed case - underscore inserted at lowercase→uppercase boundary
        assert_eq!(to_snake_case("CoNLL2003"), "co_nll2003");
        // All uppercase start - no leading underscore
        assert_eq!(to_snake_case("ACE2005"), "ace2005");
        assert_eq!(to_snake_case("NER"), "ner");
    }
}
