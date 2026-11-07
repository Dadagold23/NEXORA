use crate::ast::{Program, Statement};
use crate::diagnostic::Diagnostic;
use crate::nir::{
    split_type_arguments, stable_type_id, type_from_name_with, FunctionSignature, Type,
};
use std::collections::{HashMap, HashSet};

#[derive(Clone, Debug, PartialEq)]
pub struct HirModule {
    pub source_name: String,
    pub program: Program,
    pub functions: Vec<FunctionSignature>,
    pub type_names: Vec<String>,
}

/// Name-resolution context for one file: its dotted path plus bare-name to
/// qualified-name aliases for imported and own-module types.
#[derive(Clone, Debug, Default)]
pub struct ModuleContext {
    pub module: String,
    pub aliases: HashMap<String, String>,
}

pub fn analyze(program: &Program, source_name: impl Into<String>) -> Result<HirModule, Diagnostic> {
    analyze_module(program, source_name, &ModuleContext::default())
}

pub fn analyze_module(
    program: &Program,
    source_name: impl Into<String>,
    context: &ModuleContext,
) -> Result<HirModule, Diagnostic> {
    let type_names = program
        .statements
        .iter()
        .filter_map(declared_type_name)
        .collect::<Vec<_>>();
    // Declared generic arity per bare type name, for argument validation.
    let generic_arities = program
        .statements
        .iter()
        .filter_map(declared_type_arity)
        .collect::<HashMap<_, _>>();
    let mut seen_types = HashSet::new();
    for statement in &program.statements {
        if let Some(name) = declared_type_name(statement) {
            if !seen_types.insert(name.clone()) {
                return Err(Diagnostic::error(
                    "NXL2005",
                    format!("duplicate type declaration of '{name}'"),
                    statement_span(statement),
                ));
            }
        }
    }
    let mut functions = Vec::new();
    for statement in &program.statements {
        let Some(function) = function_declaration(statement) else {
            continue;
        };
        let Some(name) = &function.name else {
            continue;
        };
        if functions
            .iter()
            .any(|signature: &FunctionSignature| signature.name == *name)
        {
            return Err(Diagnostic::error(
                "NXL2005",
                format!("duplicate function declaration of '{name}'"),
                function.span,
            ));
        }

        let type_parameters = function.type_parameters.clone();
        let mut seen_parameters = HashSet::new();
        for parameter in &type_parameters {
            if !seen_parameters.insert(parameter) {
                return Err(Diagnostic::error(
                    "NXL2005",
                    format!("duplicate type parameter '{parameter}' in function '{name}'"),
                    function.span,
                ));
            }
        }
        let mut parameters = Vec::with_capacity(function.parameters.len());
        let mut parameter_annotations = Vec::with_capacity(function.parameters.len());
        for parameter in &function.parameters {
            let ty = match &parameter.annotation {
                Some(annotation) => resolve_type(
                    annotation,
                    &type_names,
                    &context.aliases,
                    &generic_arities,
                    &type_parameters,
                )
                .map_err(|error| type_diagnostic(error, annotation, parameter.span))?,
                None => Type::Unknown,
            };
            parameters.push((parameter.name.clone(), ty));
            parameter_annotations.push(parameter.annotation.as_ref().map(|annotation| {
                canonicalize_type_annotation(annotation, &context.aliases, &type_parameters)
            }));
        }
        let mut param_names = std::collections::HashSet::new();
        for (pname, _) in &parameters {
            if !param_names.insert(pname) {
                return Err(Diagnostic::error(
                    "NXL2005",
                    format!("duplicate parameter name '{pname}' in function '{name}'"),
                    function.span,
                ));
            }
        }
        let return_type = match &function.return_type {
            Some(annotation) => resolve_type(
                annotation,
                &type_names,
                &context.aliases,
                &generic_arities,
                &type_parameters,
            )
            .map_err(|error| type_diagnostic(error, annotation, function.span))?,
            None => Type::Unknown,
        };
        let return_annotation = function.return_type.as_ref().map(|annotation| {
            canonicalize_type_annotation(annotation, &context.aliases, &type_parameters)
        });
        functions.push(FunctionSignature {
            name: name.clone(),
            parameters,
            return_type,
            type_parameters,
            parameter_annotations,
            return_annotation,
        });
    }

    Ok(HirModule {
        source_name: source_name.into(),
        program: program.clone(),
        functions,
        type_names,
    })
}

fn function_declaration(statement: &Statement) -> Option<&crate::ast::Function> {
    match statement {
        Statement::Function(function) => Some(function),
        Statement::Export(inner) => function_declaration(inner),
        _ => None,
    }
}

fn declared_type_name(statement: &Statement) -> Option<String> {
    match statement {
        Statement::TypeDeclaration(declaration) => Some(declaration.name.clone()),
        Statement::TypeAlias(alias) => Some(alias.name.clone()),
        Statement::Export(inner) => declared_type_name(inner),
        _ => None,
    }
}

fn statement_span(statement: &Statement) -> crate::source::Span {
    match statement {
        Statement::TypeDeclaration(declaration) => declaration.span,
        Statement::TypeAlias(alias) => alias.span,
        Statement::Export(inner) => statement_span(inner),
        _ => crate::source::Span::default(),
    }
}

fn declared_type_arity(statement: &Statement) -> Option<(String, usize)> {
    match statement {
        Statement::TypeDeclaration(declaration) => {
            Some((declaration.name.clone(), declaration.type_parameters.len()))
        }
        Statement::TypeAlias(alias) => Some((alias.name.clone(), alias.type_parameters.len())),
        Statement::Export(inner) => declared_type_arity(inner),
        _ => None,
    }
}

#[derive(Debug)]
enum TypeResolveError {
    Unknown,
    Arity {
        base: String,
        expected: usize,
        found: usize,
    },
}

fn type_diagnostic(
    error: TypeResolveError,
    annotation: &str,
    span: crate::source::Span,
) -> Diagnostic {
    match error {
        TypeResolveError::Unknown => {
            Diagnostic::error("NXL2004", format!("unknown type '{annotation}'"), span)
        }
        TypeResolveError::Arity {
            base,
            expected,
            found,
        } => Diagnostic::error(
            "NXL2001",
            format!("type '{base}' expects {expected} type argument(s), found {found}"),
            span,
        ),
    }
}

/// Rewrite bare type names in an annotation to the qualified names of the
/// declaring file, leaving declared type parameters untouched. The result is
/// self-contained: call-site instantiation can substitute type arguments and
/// resolve it without the caller's (different) alias table.
pub(crate) fn canonicalize_type_annotation(
    annotation: &str,
    aliases: &HashMap<String, String>,
    parameters: &[String],
) -> String {
    let mut canonical = String::with_capacity(annotation.len());
    let mut identifier_start: Option<usize> = None;
    for (index, character) in annotation.char_indices() {
        if character.is_ascii_alphanumeric() || character == '_' {
            identifier_start.get_or_insert(index);
            continue;
        }
        if let Some(start) = identifier_start.take() {
            push_canonical_identifier(
                &mut canonical,
                &annotation[start..index],
                aliases,
                parameters,
            );
        }
        canonical.push(character);
    }
    if let Some(start) = identifier_start {
        push_canonical_identifier(&mut canonical, &annotation[start..], aliases, parameters);
    }
    canonical
}

fn push_canonical_identifier(
    out: &mut String,
    identifier: &str,
    aliases: &HashMap<String, String>,
    parameters: &[String],
) {
    if parameters.iter().any(|parameter| parameter == identifier) {
        out.push_str(identifier);
    } else if let Some(qualified) = aliases.get(identifier) {
        out.push_str(qualified);
    } else {
        out.push_str(identifier);
    }
}

fn resolve_type(
    name: &str,
    known_types: &[String],
    aliases: &HashMap<String, String>,
    generic_arities: &HashMap<String, usize>,
    params: &[String],
) -> Result<Type, TypeResolveError> {
    // A function's own type parameters are in scope before any concrete type.
    if params.iter().any(|param| param == name) {
        return Ok(Type::Param(stable_type_id(name)));
    }
    // A reference annotation (`&T`, `&mut T`) validates the type it wraps:
    // strip the wrapper before looking for a generic instantiation, so
    // `&Box<Int>` and `&[Int]` are not mistaken for types named `&Box`/`&[`.
    let checked = name
        .strip_prefix('&')
        .map(|rest| rest.strip_prefix("mut ").unwrap_or(rest))
        .unwrap_or(name)
        .trim();
    // Validate explicit instantiation of locally declared generic types.
    if let Some(open) = checked.find('<') {
        if checked.ends_with('>') {
            let base = &checked[..open];
            let is_builtin = matches!(
                base,
                "Array" | "List" | "Option" | "Task" | "Channel" | "Result" | "Map"
            );
            if !is_builtin {
                let arguments = split_type_arguments(&checked[open + 1..checked.len() - 1]);
                match generic_arities.get(base) {
                    Some(&expected) if expected != arguments.len() => {
                        return Err(TypeResolveError::Arity {
                            base: base.to_owned(),
                            expected,
                            found: arguments.len(),
                        });
                    }
                    Some(_) => {}
                    None => {
                        // Imported generic types are declared in another file;
                        // their arity is validated where they are declared.
                        if !aliases.contains_key(base) {
                            return Err(TypeResolveError::Unknown);
                        }
                    }
                }
            }
        }
    }
    type_from_name_with(name, aliases, params)
        .or_else(|| {
            aliases
                .get(name)
                .map(|qualified| Type::Generic(stable_type_id(qualified)))
        })
        .or_else(|| {
            known_types
                .iter()
                .any(|known| known == name)
                .then(|| Type::Generic(stable_type_id(name)))
        })
        .ok_or(TypeResolveError::Unknown)
}
