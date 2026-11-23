//! `types.json` (SPEC.md §7, §11; STORIES.md US-100): the inferred type of every
//! exported binding, module by module. [`TypesDocument::to_json`] writes it and
//! [`TypesDocument::from_json`] reads it back; `schema/types.schema.json` documents
//! it for other readers.

use crate::json::Json;

/// The version of the document's form.
pub const TYPES_VERSION: i64 = 1;

/// One export of a module.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExportedType {
    /// The name it is exported by.
    pub name: String,
    /// Whether inference gave it a type: when it did not, it is `unknown`.
    pub inferred: bool,
    /// Its type in the notation of SPEC.md §13.7: `greet(name: string): string`.
    pub notation: String,
    /// Its type as a tree, kind by kind.
    pub structure: Json,
}

/// The exports of one module.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModuleExports {
    /// The module, as diagnostics name it.
    pub path: String,
    /// Whether the grammar rejects the module (E000): its exports are then `unknown`.
    pub syntax_error: bool,
    /// Its exports, by name.
    pub exports: Vec<ExportedType>,
}

/// The whole document.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct TypesDocument {
    /// The modules, by path.
    pub modules: Vec<ModuleExports>,
}

impl TypesDocument {
    /// The document as JSON.
    #[must_use]
    pub fn to_json(&self) -> String {
        let modules = self.modules.iter().map(|module| {
            let exports = module.exports.iter().map(|export| {
                Json::map(vec![
                    ("name", Json::text(&export.name)),
                    ("inferred", Json::Bool(export.inferred)),
                    ("type", Json::text(&export.notation)),
                    ("structure", export.structure.clone()),
                ])
            });
            Json::map(vec![
                ("path", Json::text(&module.path)),
                ("syntaxError", Json::Bool(module.syntax_error)),
                ("exports", Json::List(exports.collect())),
            ])
        });
        let document = Json::map(vec![
            ("version", Json::Int(TYPES_VERSION)),
            ("modules", Json::List(modules.collect())),
        ]);
        document.write()
    }

    /// Reads a document written by [`TypesDocument::to_json`].
    ///
    /// # Errors
    ///
    /// What is wrong: text that is no JSON, another version, or a member that is
    /// missing or of another kind.
    pub fn from_json(text: &str) -> Result<Self, String> {
        let document = Json::read(text)?;
        if document.get("version") != Some(&Json::Int(TYPES_VERSION)) {
            return Err(format!("not version {TYPES_VERSION} of `types.json`"));
        }
        let missing = |what: &str| format!("`{what}` is missing or of another kind");
        let list = |of: &Json, key: &str| -> Result<Vec<Json>, String> {
            let list = of.get(key).and_then(Json::as_list);
            list.map(<[Json]>::to_vec).ok_or_else(|| missing(key))
        };
        let text = |of: &Json, key: &str| -> Result<String, String> {
            let text = of.get(key).and_then(Json::as_text);
            text.map(str::to_owned).ok_or_else(|| missing(key))
        };
        let flag = |of: &Json, key: &str| -> Result<bool, String> {
            of.get(key)
                .and_then(Json::as_bool)
                .ok_or_else(|| missing(key))
        };
        let mut modules = Vec::new();
        for module in list(&document, "modules")? {
            let mut exports = Vec::new();
            for export in list(&module, "exports")? {
                let structure = export.get("structure").cloned();
                exports.push(ExportedType {
                    name: text(&export, "name")?,
                    inferred: flag(&export, "inferred")?,
                    notation: text(&export, "type")?,
                    structure: structure.ok_or_else(|| missing("structure"))?,
                });
            }
            modules.push(ModuleExports {
                path: text(&module, "path")?,
                syntax_error: flag(&module, "syntaxError")?,
                exports,
            });
        }
        Ok(Self { modules })
    }
}
