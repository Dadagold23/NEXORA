use crate::ast::{Program, Statement};
use crate::diagnostic::Diagnostic;
use crate::nir::{stable_type_id, type_from_name, FunctionSignature, Type};
use std::collections::HashSet;

#[derive(Clone, Debug, PartialEq)]
pub struct HirModule {
    pub source_name: String,
    pub program: Program,
    pub functions: Vec<FunctionSignature>,
    pub type_names: Vec<String>,
}

pub fn analyze(program: &Program, source_name: impl Into<String>) -> Result<HirModule, Diagnostic> {
    let type_names = program
        .statements
        .iter()
        .filter_map(declared_type_name)
        .collect::<Vec<_>>();
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

        let mut parameters = Vec::with_capacity(function.parameters.len());
        for parameter in &function.parameters {
            let ty = match &parameter.annotation {
                Some(annotation) => resolve_type(annotation, &type_names).ok_or_else(|| {
                    Diagnostic::error(
                        "NXL2004",
                        format!("unknown type '{annotation}'"),
                        parameter.span,
                    )
                })?,
                None => Type::Unknown,
            };
            parameters.push((parameter.name.clone(), ty));
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
            Some(annotation) => resolve_type(annotation, &type_names).ok_or_else(|| {
                Diagnostic::error(
                    "NXL2004",
                    format!("unknown type '{annotation}'"),
                    function.span,
                )
            })?,
            None => Type::Unknown,
        };
        functions.push(FunctionSignature {
            name: name.clone(),
            parameters,
            return_type,
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

fn resolve_type(name: &str, known_types: &[String]) -> Option<Type> {
    type_from_name(name).or_else(|| {
        known_types
            .iter()
            .any(|known| known == name)
            .then(|| Type::Generic(stable_type_id(name)))
    })
}
