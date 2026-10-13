use crate::ast::{
    AssignmentOperator, BinaryOperator, Block, CatchClause, DeclarationKind, EnumVariant,
    Expression, ExpressionKind, Field, Function, GenericConstraint, ImplBlock, MatchBody,
    MatchCase, Parameter, Pattern, PatternKind, Program, Statement, TypeAlias, TypeDeclaration,
    UnaryOperator,
};
use crate::diagnostic::Diagnostic;
use crate::lexer::{Token, TokenKind, TokenStringPart};
use crate::source::Span;

pub struct Parser {
    tokens: Vec<Token>,
    current: usize,
}

impl Parser {
    pub fn new(tokens: Vec<Token>) -> Self {
        Self { tokens, current: 0 }
    }

    pub fn parse(mut self) -> Result<Program, Diagnostic> {
        let mut statements = Vec::new();
        while !self.check(&TokenKind::Eof) {
            statements.push(self.statement()?);
        }
        Ok(Program { statements })
    }

    pub fn parse_expression_only(mut self) -> Result<Expression, Diagnostic> {
        let expression = self.expression()?;
        if !self.check(&TokenKind::Eof) {
            let token = self.peek().clone();
            return Err(self.error_at(&token, "unexpected token after expression"));
        }
        Ok(expression)
    }

    fn statement(&mut self) -> Result<Statement, Diagnostic> {
        if self.matches(&TokenKind::Export) {
            let statement = self.statement()?;
            return Ok(Statement::Export(Box::new(statement)));
        }
        if self.matches(&TokenKind::Let) || self.matches(&TokenKind::Const) {
            return self.variable_declaration();
        }
        if self.matches(&TokenKind::Fn) {
            return Ok(Statement::Function(self.function(false, true, true)?));
        }
        if self.check(&TokenKind::Async) && self.check_at(1, &TokenKind::Fn) {
            self.advance();
            self.advance();
            return Ok(Statement::Function(self.function(true, true, true)?));
        }
        if self.matches(&TokenKind::Struct) {
            return Ok(Statement::TypeDeclaration(
                self.type_declaration(DeclarationKind::Struct)?,
            ));
        }
        if self.matches(&TokenKind::Class) {
            return Ok(Statement::TypeDeclaration(
                self.type_declaration(DeclarationKind::Class)?,
            ));
        }
        if self.matches(&TokenKind::Actor) {
            return Ok(Statement::TypeDeclaration(
                self.type_declaration(DeclarationKind::Actor)?,
            ));
        }
        if self.matches(&TokenKind::Interface) {
            return Ok(Statement::TypeDeclaration(
                self.type_declaration(DeclarationKind::Interface)?,
            ));
        }
        if self.matches(&TokenKind::Trait) {
            return Ok(Statement::TypeDeclaration(
                self.type_declaration(DeclarationKind::Trait)?,
            ));
        }
        if self.matches(&TokenKind::Enum) {
            return Ok(Statement::TypeDeclaration(
                self.type_declaration(DeclarationKind::Enum)?,
            ));
        }
        if self.matches(&TokenKind::Type) {
            let start = self.previous().span;
            let name = self.expect_identifier("expected type alias name")?;
            let type_parameters = self.type_parameters()?;
            self.consume(&TokenKind::Equal, "expected '=' in type alias")?;
            let target = self.type_annotation()?;
            let end = self.consume_optional_semicolon();
            return Ok(Statement::TypeAlias(TypeAlias {
                name,
                type_parameters,
                target,
                span: start.join(end),
            }));
        }
        if self.matches(&TokenKind::Impl) {
            return self.impl_block();
        }
        if self.matches(&TokenKind::Module) {
            let start = self.previous().span;
            let path = self.path()?;
            let end = self.consume_optional_semicolon();
            return Ok(Statement::Module {
                path,
                span: start.join(end),
            });
        }
        if self.matches(&TokenKind::Import) {
            return self.import_declaration();
        }
        if self.matches(&TokenKind::From) {
            return self.selective_import();
        }
        if self.matches(&TokenKind::If) {
            return self.if_statement();
        }
        if self.matches(&TokenKind::For) {
            return self.for_statement();
        }
        if self.matches(&TokenKind::While) {
            return self.while_statement();
        }
        if self.matches(&TokenKind::Loop) {
            let start = self.previous().span;
            let body = self.block()?;
            return Ok(Statement::Loop {
                span: start.join(body.span),
                body,
            });
        }
        if self.matches(&TokenKind::Break) {
            let span = self.previous().span.join(self.consume_optional_semicolon());
            return Ok(Statement::Break(span));
        }
        if self.matches(&TokenKind::Continue) {
            let span = self.previous().span.join(self.consume_optional_semicolon());
            return Ok(Statement::Continue(span));
        }
        if self.matches(&TokenKind::Return) {
            let start = self.previous().span;
            let value = if self.is_statement_end() {
                None
            } else {
                Some(self.expression()?)
            };
            let end = self.consume_optional_semicolon();
            return Ok(Statement::Return {
                value,
                span: start.join(end),
            });
        }
        if self.matches(&TokenKind::Throw) {
            let start = self.previous().span;
            let value = self.expression()?;
            let end = self.consume_optional_semicolon();
            return Ok(Statement::Throw {
                span: start.join(end),
                value,
            });
        }
        if self.matches(&TokenKind::Yield) {
            let start = self.previous().span;
            let value = if self.is_statement_end() {
                None
            } else {
                Some(self.expression()?)
            };
            let end = self.consume_optional_semicolon();
            return Ok(Statement::Yield {
                value,
                span: start.join(end),
            });
        }
        if self.matches(&TokenKind::Try) {
            return self.try_statement();
        }
        if self.matches(&TokenKind::Unsafe) {
            return Ok(Statement::Unsafe(self.block()?));
        }
        if self.matches(&TokenKind::Match) {
            return self.match_statement();
        }
        if self.matches(&TokenKind::LeftBrace) {
            let start = self.previous().span;
            return self.block_after_open(start).map(Statement::Block);
        }

        let expression = self.expression()?;
        self.matches(&TokenKind::Semicolon);
        Ok(Statement::Expression(expression))
    }

    fn variable_declaration(&mut self) -> Result<Statement, Diagnostic> {
        let keyword = self.previous().clone();
        let is_const = keyword.kind == TokenKind::Const;
        let is_mut = self.matches(&TokenKind::Mut);
        let name_token = self.advance().clone();
        let TokenKind::Identifier(name) = &name_token.kind else {
            return Err(self.error_at(&name_token, "expected a variable name"));
        };
        let name = name.clone();
        let annotation = if self.matches(&TokenKind::Colon) {
            Some(self.type_annotation()?)
        } else {
            None
        };
        let value = if self.matches(&TokenKind::Equal) {
            Some(self.expression()?)
        } else {
            None
        };
        if is_const && value.is_none() {
            return Err(self.error_at(&keyword, "constant declarations require an initializer"));
        }
        if annotation.is_none() && value.is_none() {
            return Err(self.error_at(
                &name_token,
                "declaration requires a type annotation or initializer",
            ));
        }
        let end = self.consume_optional_semicolon();
        Ok(Statement::Let {
            name,
            annotation,
            is_const,
            is_mut,
            value,
            span: keyword.span.join(end),
        })
    }

    fn function(
        &mut self,
        is_async: bool,
        allow_name: bool,
        allow_expression_body: bool,
    ) -> Result<Function, Diagnostic> {
        let start = self.previous().span;
        let name = if allow_name && !self.check(&TokenKind::LeftParen) {
            Some(self.expect_identifier("expected function name")?)
        } else {
            None
        };
        let type_parameters = self.type_parameters()?;
        self.consume(&TokenKind::LeftParen, "expected '(' after function name")?;
        let parameters = self.parameters()?;
        self.consume(&TokenKind::RightParen, "expected ')' after parameters")?;
        let return_type = if self.matches(&TokenKind::Arrow) {
            Some(self.type_annotation()?)
        } else {
            None
        };
        let constraints = self.where_clause()?;
        let body = if allow_expression_body && self.matches(&TokenKind::FatArrow) {
            let value = self.expression()?;
            let body_span = start.join(value.span);
            Some(Block {
                statements: vec![Statement::Return {
                    span: value.span,
                    value: Some(value),
                }],
                span: body_span,
            })
        } else if self.check(&TokenKind::LeftBrace) {
            Some(self.block()?)
        } else {
            None
        };
        if body.is_none() && !self.check(&TokenKind::Semicolon) {
            let token = self.peek().clone();
            return Err(self.error_at(&token, "expected a function body"));
        }
        let end = body
            .as_ref()
            .map_or_else(|| self.previous().span, |body| body.span);
        self.matches(&TokenKind::Semicolon);
        Ok(Function {
            name,
            type_parameters,
            constraints,
            parameters,
            return_type,
            body,
            is_async,
            span: start.join(end),
        })
    }

    fn parameters(&mut self) -> Result<Vec<Parameter>, Diagnostic> {
        let mut parameters = Vec::new();
        if self.check(&TokenKind::RightParen) {
            return Ok(parameters);
        }
        loop {
            let name_token = self.advance().clone();
            let TokenKind::Identifier(name) = name_token.kind else {
                return Err(self.error_at(&name_token, "expected parameter name"));
            };
            let annotation = if self.matches(&TokenKind::Colon) {
                Some(self.type_annotation()?)
            } else {
                None
            };
            let end = self.previous().span;
            parameters.push(Parameter {
                name,
                annotation,
                span: name_token.span.join(end),
            });
            if !self.matches(&TokenKind::Comma) {
                break;
            }
        }
        Ok(parameters)
    }

    fn where_clause(&mut self) -> Result<Vec<GenericConstraint>, Diagnostic> {
        if !self.matches(&TokenKind::Where) {
            return Ok(Vec::new());
        }
        let mut constraints = Vec::new();
        loop {
            let parameter = self.expect_identifier("expected constrained type parameter")?;
            let start = self.previous().span;
            self.consume(
                &TokenKind::Colon,
                "expected ':' after constrained type parameter",
            )?;
            let mut bounds = vec![self.expect_identifier("expected trait bound")?];
            while self.matches(&TokenKind::Plus) || self.matches(&TokenKind::Ampersand) {
                bounds.push(self.expect_identifier("expected trait bound")?);
            }
            let end = self.previous().span;
            constraints.push(GenericConstraint {
                parameter,
                bounds,
                span: start.join(end),
            });
            if !self.matches(&TokenKind::Comma) {
                break;
            }
        }
        Ok(constraints)
    }

    fn type_parameters(&mut self) -> Result<Vec<String>, Diagnostic> {
        if !self.matches(&TokenKind::Less) {
            return Ok(Vec::new());
        }
        let mut parameters = Vec::new();
        loop {
            parameters.push(self.expect_identifier("expected type parameter name")?);
            if !self.matches(&TokenKind::Comma) {
                break;
            }
        }
        self.consume_type_greater("expected '>' after type parameters")?;
        Ok(parameters)
    }

    fn type_declaration(&mut self, kind: DeclarationKind) -> Result<TypeDeclaration, Diagnostic> {
        let start = self.previous().span;
        let name = self.expect_identifier("expected type declaration name")?;
        let type_parameters = self.type_parameters()?;
        let constraints = self.where_clause()?;
        let constructor_parameters =
            if kind == DeclarationKind::Class && self.matches(&TokenKind::LeftParen) {
                let parameters = self.parameters()?;
                self.consume(
                    &TokenKind::RightParen,
                    "expected ')' after class constructor parameters",
                )?;
                parameters
            } else {
                Vec::new()
            };
        self.consume(&TokenKind::LeftBrace, "expected '{' after type name")?;
        let mut fields = Vec::new();
        let mut methods = Vec::new();
        let mut variants = Vec::new();
        while !self.check(&TokenKind::RightBrace) && !self.check(&TokenKind::Eof) {
            if kind == DeclarationKind::Enum {
                let variant_token = self.advance().clone();
                let TokenKind::Identifier(variant_name) = variant_token.kind else {
                    return Err(self.error_at(&variant_token, "expected enum variant name"));
                };
                let mut payload = Vec::new();
                if self.matches(&TokenKind::LeftParen) {
                    if !self.check(&TokenKind::RightParen) {
                        loop {
                            payload.push(self.type_annotation()?);
                            if !self.matches(&TokenKind::Comma) {
                                break;
                            }
                        }
                    }
                    self.consume(&TokenKind::RightParen, "expected ')' after enum payload")?;
                }
                let end = self.consume_optional_semicolon();
                self.matches(&TokenKind::Comma);
                variants.push(EnumVariant {
                    name: variant_name,
                    payload,
                    span: variant_token.span.join(end),
                });
                continue;
            }

            let is_async = self.matches(&TokenKind::Async);
            if self.matches(&TokenKind::Fn) {
                methods.push(self.function(is_async, true, true)?);
                continue;
            }
            if is_async {
                let token = self.previous().clone();
                return Err(self.error_at(&token, "expected 'fn' after 'async'"));
            }
            if self.matches(&TokenKind::Let) || self.matches(&TokenKind::Const) {
                let keyword = self.previous().clone();
                let is_const = keyword.kind == TokenKind::Const;
                let name_token = self.advance().clone();
                let TokenKind::Identifier(field_name) = name_token.kind else {
                    return Err(self.error_at(&name_token, "expected field name"));
                };
                self.consume(&TokenKind::Colon, "expected ':' after field name")?;
                let annotation = self.type_annotation()?;
                let value = if self.matches(&TokenKind::Equal) {
                    Some(self.expression()?)
                } else {
                    None
                };
                let end = self.consume_optional_semicolon();
                fields.push(Field {
                    name: field_name,
                    annotation,
                    value,
                    is_const,
                    span: keyword.span.join(end),
                });
                continue;
            }

            if matches!(
                kind,
                DeclarationKind::Struct | DeclarationKind::Interface | DeclarationKind::Trait
            ) {
                let field_token = self.advance().clone();
                let TokenKind::Identifier(field_name) = field_token.kind else {
                    return Err(self.error_at(&field_token, "expected field name"));
                };
                self.consume(&TokenKind::Colon, "expected ':' after field name")?;
                let annotation = self.type_annotation()?;
                let end = self.consume_optional_semicolon();
                fields.push(Field {
                    name: field_name,
                    annotation,
                    value: None,
                    is_const: true,
                    span: field_token.span.join(end),
                });
                continue;
            }

            let token = self.peek().clone();
            return Err(self.error_at(&token, "expected a field or function declaration"));
        }
        let end = self.consume(&TokenKind::RightBrace, "expected '}' after declaration")?;
        Ok(TypeDeclaration {
            kind,
            name,
            type_parameters,
            constraints,
            constructor_parameters,
            fields,
            methods,
            variants,
            span: start.join(end.span),
        })
    }

    fn impl_block(&mut self) -> Result<Statement, Diagnostic> {
        let start = self.previous().span;
        let first_name = self.expect_identifier("expected trait or type name after 'impl'")?;
        let (trait_name, target) = if self.matches(&TokenKind::For) {
            (
                Some(first_name),
                self.expect_identifier("expected target type after 'for'")?,
            )
        } else {
            (None, first_name)
        };
        self.consume(&TokenKind::LeftBrace, "expected '{' after impl target")?;
        let mut methods = Vec::new();
        while !self.check(&TokenKind::RightBrace) && !self.check(&TokenKind::Eof) {
            let is_async = self.matches(&TokenKind::Async);
            self.consume(&TokenKind::Fn, "expected function in impl block")?;
            methods.push(self.function(is_async, true, true)?);
        }
        let end = self.consume(&TokenKind::RightBrace, "expected '}' after impl block")?;
        Ok(Statement::ImplBlock(ImplBlock {
            trait_name,
            target,
            methods,
            span: start.join(end.span),
        }))
    }

    fn import_declaration(&mut self) -> Result<Statement, Diagnostic> {
        let start = self.previous().span;
        let path = self.path()?;
        let alias = if self.matches(&TokenKind::As) {
            Some(self.expect_identifier("expected import alias")?)
        } else {
            None
        };
        let end = self.consume_optional_semicolon();
        Ok(Statement::Import {
            path,
            alias,
            selected: Vec::new(),
            span: start.join(end),
        })
    }

    fn selective_import(&mut self) -> Result<Statement, Diagnostic> {
        let start = self.previous().span;
        let path = self.path()?;
        self.consume(&TokenKind::Import, "expected 'import' after module path")?;
        let mut selected = Vec::new();
        loop {
            selected.push(self.expect_identifier("expected imported name")?);
            if !self.matches(&TokenKind::Comma) {
                break;
            }
        }
        let end = self.consume_optional_semicolon();
        Ok(Statement::Import {
            path,
            alias: None,
            selected,
            span: start.join(end),
        })
    }

    fn path(&mut self) -> Result<Vec<String>, Diagnostic> {
        let mut path = vec![self.expect_identifier("expected path name")?];
        while self.matches(&TokenKind::Dot) {
            path.push(self.expect_identifier("expected name after '.'")?);
        }
        Ok(path)
    }

    fn if_statement(&mut self) -> Result<Statement, Diagnostic> {
        let start = self.previous().span;
        let condition = self.expression()?;
        let then_block = self.block()?;
        let else_branch = if self.matches(&TokenKind::Else) {
            if self.matches(&TokenKind::If) {
                Some(Box::new(self.if_statement()?))
            } else {
                Some(Box::new(Statement::Block(self.block()?)))
            }
        } else {
            None
        };
        let end = else_branch
            .as_ref()
            .map_or(then_block.span, |branch| statement_span(branch));
        Ok(Statement::If {
            condition,
            then_block,
            else_branch,
            span: start.join(end),
        })
    }

    fn for_statement(&mut self) -> Result<Statement, Diagnostic> {
        let start = self.previous().span;
        let binding = self.expect_identifier("expected loop binding")?;
        self.consume(&TokenKind::In, "expected 'in' after loop binding")?;
        let iterable = self.expression()?;
        let body = self.block()?;
        Ok(Statement::For {
            binding,
            iterable,
            span: start.join(body.span),
            body,
        })
    }

    fn while_statement(&mut self) -> Result<Statement, Diagnostic> {
        let start = self.previous().span;
        let condition = self.expression()?;
        let body = self.block()?;
        Ok(Statement::While {
            condition,
            span: start.join(body.span),
            body,
        })
    }

    fn try_statement(&mut self) -> Result<Statement, Diagnostic> {
        let start = self.previous().span;
        let body = self.block()?;
        let mut catches = Vec::new();
        while self.matches(&TokenKind::Catch) {
            let catch_start = self.previous().span;
            let name = self.expect_identifier("expected catch binding")?;
            let annotation = if self.matches(&TokenKind::Colon) {
                Some(self.type_annotation()?)
            } else {
                None
            };
            let catch_body = self.block()?;
            catches.push(CatchClause {
                name,
                annotation,
                span: catch_start.join(catch_body.span),
                body: catch_body,
            });
        }
        let finally = if self.matches(&TokenKind::Finally) {
            Some(self.block()?)
        } else {
            None
        };
        if catches.is_empty() && finally.is_none() {
            let token = self.peek().clone();
            return Err(self.error_at(&token, "try requires a catch or finally clause"));
        }
        let end = finally.as_ref().map_or_else(
            || catches.last().map_or(body.span, |item| item.span),
            |block| block.span,
        );
        Ok(Statement::Try {
            body,
            catches,
            finally,
            span: start.join(end),
        })
    }

    fn match_statement(&mut self) -> Result<Statement, Diagnostic> {
        let start = self.previous().span;
        let value = self.expression()?;
        let cases = self.match_cases()?;
        let end = cases.last().map_or(value.span, |case| case.span);
        Ok(Statement::Match {
            value,
            cases,
            span: start.join(end),
        })
    }

    fn match_cases(&mut self) -> Result<Vec<MatchCase>, Diagnostic> {
        self.consume(&TokenKind::LeftBrace, "expected '{' before match cases")?;
        let mut cases = Vec::new();
        while !self.check(&TokenKind::RightBrace) && !self.check(&TokenKind::Eof) {
            let start = self
                .consume(&TokenKind::Case, "expected 'case' in match body")?
                .span;
            let pattern = self.pattern()?;
            let guard = if self.matches(&TokenKind::When) {
                Some(self.expression()?)
            } else {
                None
            };
            let body = if self.matches(&TokenKind::FatArrow) {
                let expression = self.expression()?;
                self.matches(&TokenKind::Semicolon);
                MatchBody::Expression(expression)
            } else {
                MatchBody::Block(self.block()?)
            };
            let end = match &body {
                MatchBody::Expression(expression) => expression.span,
                MatchBody::Block(block) => block.span,
            };
            cases.push(MatchCase {
                pattern,
                guard,
                body,
                span: start.join(end),
            });
        }
        self.consume(&TokenKind::RightBrace, "expected '}' after match cases")?;
        if cases.is_empty() {
            let token = self.previous().clone();
            return Err(self.error_at(&token, "match requires at least one case"));
        }
        Ok(cases)
    }

    fn pattern(&mut self) -> Result<Pattern, Diagnostic> {
        let mut alternatives = vec![self.pattern_primary()?];
        while self.matches(&TokenKind::Pipe) {
            alternatives.push(self.pattern_primary()?);
        }
        if alternatives.len() == 1 {
            Ok(alternatives.remove(0))
        } else {
            let span = alternatives
                .first()
                .unwrap()
                .span
                .join(alternatives.last().unwrap().span);
            Ok(Pattern {
                kind: PatternKind::Or(alternatives),
                span,
            })
        }
    }

    fn pattern_primary(&mut self) -> Result<Pattern, Diagnostic> {
        let token = self.advance().clone();
        let kind = match token.kind {
            TokenKind::Integer(value) => PatternKind::Literal(Box::new(
                self.literal_expression(ExpressionKind::Integer(value), token.span),
            )),
            TokenKind::Float(value) => PatternKind::Literal(Box::new(
                self.literal_expression(ExpressionKind::Float(value), token.span),
            )),
            TokenKind::String(value) => PatternKind::Literal(Box::new(
                self.literal_expression(ExpressionKind::String(value), token.span),
            )),
            TokenKind::Character(value) => PatternKind::Literal(Box::new(
                self.literal_expression(ExpressionKind::Character(value), token.span),
            )),
            TokenKind::True => PatternKind::Literal(Box::new(
                self.literal_expression(ExpressionKind::Boolean(true), token.span),
            )),
            TokenKind::False => PatternKind::Literal(Box::new(
                self.literal_expression(ExpressionKind::Boolean(false), token.span),
            )),
            TokenKind::Null => PatternKind::Literal(Box::new(
                self.literal_expression(ExpressionKind::Null, token.span),
            )),
            TokenKind::Identifier(name) if name == "_" => PatternKind::Wildcard,
            TokenKind::Identifier(name) if self.matches(&TokenKind::LeftParen) => {
                let fields = self.pattern_list(&TokenKind::RightParen)?;
                PatternKind::Variant { name, fields }
            }
            TokenKind::Identifier(name) if self.matches(&TokenKind::LeftBrace) => {
                let mut fields = Vec::new();
                while !self.check(&TokenKind::RightBrace) && !self.check(&TokenKind::Eof) {
                    let field_name = self.expect_identifier("expected struct pattern field")?;
                    self.consume(&TokenKind::Colon, "expected ':' after struct pattern field")?;
                    fields.push((field_name, self.pattern()?));
                    if !self.matches(&TokenKind::Comma) {
                        break;
                    }
                }
                self.consume(&TokenKind::RightBrace, "expected '}' after struct pattern")?;
                PatternKind::Struct { name, fields }
            }
            TokenKind::Identifier(name) => PatternKind::Binding(name),
            TokenKind::LeftParen => PatternKind::Tuple(self.pattern_list(&TokenKind::RightParen)?),
            TokenKind::LeftBracket => {
                PatternKind::Array(self.pattern_list(&TokenKind::RightBracket)?)
            }
            _ => return Err(self.error_at(&token, "expected a pattern")),
        };
        let end = self.previous().span;
        Ok(Pattern {
            kind,
            span: token.span.join(end),
        })
    }

    fn pattern_list(&mut self, closing: &TokenKind) -> Result<Vec<Pattern>, Diagnostic> {
        let mut patterns = Vec::new();
        if !self.check(closing) {
            loop {
                patterns.push(self.pattern()?);
                if !self.matches(&TokenKind::Comma) {
                    break;
                }
            }
        }
        self.consume(closing, "expected closing delimiter after pattern")?;
        Ok(patterns)
    }

    fn block(&mut self) -> Result<Block, Diagnostic> {
        let start = self.consume(&TokenKind::LeftBrace, "expected '{'")?.span;
        self.block_after_open(start)
    }

    fn block_after_open(&mut self, start: Span) -> Result<Block, Diagnostic> {
        let mut statements = Vec::new();
        while !self.check(&TokenKind::RightBrace) && !self.check(&TokenKind::Eof) {
            statements.push(self.statement()?);
        }
        let end = self.consume(&TokenKind::RightBrace, "expected '}' after block")?;
        Ok(Block {
            statements,
            span: start.join(end.span),
        })
    }

    fn type_annotation(&mut self) -> Result<String, Diagnostic> {
        if self.matches(&TokenKind::LeftBracket) {
            let inner = self.type_annotation()?;
            self.consume(&TokenKind::RightBracket, "expected ']' after array type")?;
            return Ok(format!("Array<{inner}>"));
        }
        if self.matches(&TokenKind::LeftParen) {
            let first = self.type_annotation()?;
            if !self.matches(&TokenKind::Comma) {
                self.consume(&TokenKind::RightParen, "expected ')' after type")?;
                return Ok(first);
            }
            let mut types = vec![first, self.type_annotation()?];
            while self.matches(&TokenKind::Comma) {
                types.push(self.type_annotation()?);
            }
            self.consume(&TokenKind::RightParen, "expected ')' after tuple type")?;
            return Ok(format!("({})", types.join(", ")));
        }

        let mut name = self.expect_identifier("expected type name")?;
        while self.matches(&TokenKind::Dot) {
            name.push('.');
            name.push_str(&self.expect_identifier("expected type name after '.'")?);
        }
        if self.matches(&TokenKind::Less) {
            let mut arguments = vec![self.type_annotation()?];
            while self.matches(&TokenKind::Comma) {
                arguments.push(self.type_annotation()?);
            }
            self.consume_type_greater("expected '>' after generic type arguments")?;
            name.push('<');
            name.push_str(&arguments.join(", "));
            name.push('>');
        }
        if self.matches(&TokenKind::Question) {
            name.push('?');
        }
        Ok(name)
    }

    fn expression(&mut self) -> Result<Expression, Diagnostic> {
        self.assignment()
    }

    fn assignment(&mut self) -> Result<Expression, Diagnostic> {
        let target = self.conditional()?;
        let operator = match self.peek().kind {
            TokenKind::Equal => Some(AssignmentOperator::Assign),
            TokenKind::PlusEqual => Some(AssignmentOperator::Add),
            TokenKind::MinusEqual => Some(AssignmentOperator::Subtract),
            TokenKind::StarEqual => Some(AssignmentOperator::Multiply),
            TokenKind::SlashEqual => Some(AssignmentOperator::Divide),
            TokenKind::PercentEqual => Some(AssignmentOperator::Remainder),
            TokenKind::AndEqual => Some(AssignmentOperator::And),
            TokenKind::OrEqual => Some(AssignmentOperator::Or),
            _ => None,
        };
        if let Some(operator) = operator {
            self.advance();
            let value = self.assignment()?;
            let span = target.span.join(value.span);
            return Ok(Expression {
                kind: ExpressionKind::Assignment {
                    target: Box::new(target),
                    operator,
                    value: Box::new(value),
                },
                span,
            });
        }
        Ok(target)
    }

    fn conditional(&mut self) -> Result<Expression, Diagnostic> {
        let condition = self.logical_or()?;
        if self.matches(&TokenKind::Question) {
            let then_value = self.expression()?;
            self.consume(&TokenKind::Colon, "expected ':' in conditional expression")?;
            let else_value = self.conditional()?;
            let span = condition.span.join(else_value.span);
            return Ok(Expression {
                kind: ExpressionKind::Conditional {
                    condition: Box::new(condition),
                    then_value: Box::new(then_value),
                    else_value: Box::new(else_value),
                },
                span,
            });
        }
        Ok(condition)
    }

    fn logical_or(&mut self) -> Result<Expression, Diagnostic> {
        self.binary(
            Self::logical_and,
            &[
                (TokenKind::Or, BinaryOperator::Or),
                (TokenKind::OrOr, BinaryOperator::Or),
            ],
        )
    }

    fn logical_and(&mut self) -> Result<Expression, Diagnostic> {
        self.binary(
            Self::equality,
            &[
                (TokenKind::And, BinaryOperator::And),
                (TokenKind::AndAnd, BinaryOperator::And),
            ],
        )
    }

    fn bitwise_or(&mut self) -> Result<Expression, Diagnostic> {
        self.binary(
            Self::bitwise_xor,
            &[(TokenKind::Pipe, BinaryOperator::BitOr)],
        )
    }

    fn bitwise_xor(&mut self) -> Result<Expression, Diagnostic> {
        self.binary(
            Self::bitwise_and,
            &[(TokenKind::Caret, BinaryOperator::BitXor)],
        )
    }

    fn bitwise_and(&mut self) -> Result<Expression, Diagnostic> {
        self.binary(
            Self::comparison,
            &[(TokenKind::Ampersand, BinaryOperator::BitAnd)],
        )
    }

    fn equality(&mut self) -> Result<Expression, Diagnostic> {
        self.binary(
            Self::bitwise_or,
            &[
                (TokenKind::EqualEqual, BinaryOperator::Equal),
                (TokenKind::BangEqual, BinaryOperator::NotEqual),
            ],
        )
    }

    fn comparison(&mut self) -> Result<Expression, Diagnostic> {
        self.binary(
            Self::shift,
            &[
                (TokenKind::Less, BinaryOperator::Less),
                (TokenKind::LessEqual, BinaryOperator::LessEqual),
                (TokenKind::Greater, BinaryOperator::Greater),
                (TokenKind::GreaterEqual, BinaryOperator::GreaterEqual),
                (TokenKind::In, BinaryOperator::In),
                (TokenKind::Is, BinaryOperator::Is),
                (TokenKind::Range, BinaryOperator::Range),
            ],
        )
    }

    fn shift(&mut self) -> Result<Expression, Diagnostic> {
        self.binary(
            Self::term,
            &[
                (TokenKind::ShiftLeft, BinaryOperator::ShiftLeft),
                (TokenKind::ShiftRight, BinaryOperator::ShiftRight),
            ],
        )
    }

    fn term(&mut self) -> Result<Expression, Diagnostic> {
        self.binary(
            Self::factor,
            &[
                (TokenKind::Plus, BinaryOperator::Add),
                (TokenKind::Minus, BinaryOperator::Subtract),
            ],
        )
    }

    fn factor(&mut self) -> Result<Expression, Diagnostic> {
        self.binary(
            Self::unary,
            &[
                (TokenKind::Star, BinaryOperator::Multiply),
                (TokenKind::Slash, BinaryOperator::Divide),
                (TokenKind::Percent, BinaryOperator::Remainder),
            ],
        )
    }

    fn binary(
        &mut self,
        next: fn(&mut Self) -> Result<Expression, Diagnostic>,
        operators: &[(TokenKind, BinaryOperator)],
    ) -> Result<Expression, Diagnostic> {
        let mut expression = next(self)?;
        while let Some(operator) = operators
            .iter()
            .find_map(|(kind, operator)| self.check(kind).then_some(*operator))
        {
            self.advance();
            let right = next(self)?;
            let span = expression.span.join(right.span);
            expression = Expression {
                kind: ExpressionKind::Binary {
                    left: Box::new(expression),
                    operator,
                    right: Box::new(right),
                },
                span,
            };
        }
        Ok(expression)
    }

    fn unary(&mut self) -> Result<Expression, Diagnostic> {
        let operator = match self.peek().kind {
            TokenKind::Minus => Some(UnaryOperator::Negate),
            TokenKind::Bang | TokenKind::Not => Some(UnaryOperator::Not),
            TokenKind::Ref => Some(UnaryOperator::Reference),
            TokenKind::Borrow => Some(UnaryOperator::Borrow),
            _ => None,
        };
        if let Some(operator) = operator {
            let start = self.advance().span;
            let operand = self.unary()?;
            return Ok(Expression {
                span: start.join(operand.span),
                kind: ExpressionKind::Unary {
                    operator,
                    operand: Box::new(operand),
                },
            });
        }
        if self.matches(&TokenKind::Await) || self.matches(&TokenKind::Spawn) {
            let token = self.previous().clone();
            let operand = self.unary()?;
            let span = token.span.join(operand.span);
            let kind = if token.kind == TokenKind::Await {
                ExpressionKind::Await(Box::new(operand))
            } else {
                ExpressionKind::Spawn(Box::new(operand))
            };
            return Ok(Expression { kind, span });
        }
        self.postfix()
    }

    fn postfix(&mut self) -> Result<Expression, Diagnostic> {
        let mut expression = self.primary()?;
        loop {
            if self.matches(&TokenKind::LeftParen) {
                let mut arguments = Vec::new();
                if !self.check(&TokenKind::RightParen) {
                    loop {
                        arguments.push(self.expression()?);
                        if !self.matches(&TokenKind::Comma) {
                            break;
                        }
                    }
                }
                let end = self.consume(&TokenKind::RightParen, "expected ')' after arguments")?;
                let span = expression.span.join(end.span);
                expression = Expression {
                    kind: ExpressionKind::Call {
                        callee: Box::new(expression),
                        arguments,
                    },
                    span,
                };
            } else if self.matches(&TokenKind::Dot) || self.matches(&TokenKind::NullSafeDot) {
                let null_safe = self.previous().kind == TokenKind::NullSafeDot;
                let member = self.expect_identifier("expected member name after '.'")?;
                let span = expression.span.join(self.previous().span);
                expression = Expression {
                    kind: ExpressionKind::Member {
                        target: Box::new(expression),
                        name: member,
                        null_safe,
                    },
                    span,
                };
            } else if self.matches(&TokenKind::LeftBracket) {
                let index = self.expression()?;
                let end = self.consume(&TokenKind::RightBracket, "expected ']' after index")?;
                let span = expression.span.join(end.span);
                expression = Expression {
                    kind: ExpressionKind::Index {
                        target: Box::new(expression),
                        index: Box::new(index),
                    },
                    span,
                };
            } else {
                break;
            }
        }
        Ok(expression)
    }

    fn primary(&mut self) -> Result<Expression, Diagnostic> {
        if self.check(&TokenKind::Eof) {
            let token = self.peek().clone();
            return Err(self.error_at(&token, "expected an expression"));
        }
        let token = self.advance().clone();
        match token.kind {
            TokenKind::Integer(value) => {
                Ok(self.literal_expression(ExpressionKind::Integer(value), token.span))
            }
            TokenKind::Float(value) => {
                Ok(self.literal_expression(ExpressionKind::Float(value), token.span))
            }
            TokenKind::String(value) => {
                Ok(self.literal_expression(ExpressionKind::String(value), token.span))
            }
            TokenKind::Character(value) => {
                Ok(self.literal_expression(ExpressionKind::Character(value), token.span))
            }
            TokenKind::StringTemplate(parts) => self.interpolated_string(parts, token.span),
            TokenKind::True => {
                Ok(self.literal_expression(ExpressionKind::Boolean(true), token.span))
            }
            TokenKind::False => {
                Ok(self.literal_expression(ExpressionKind::Boolean(false), token.span))
            }
            TokenKind::Null => Ok(self.literal_expression(ExpressionKind::Null, token.span)),
            TokenKind::Identifier(name) => {
                if self.is_object_literal_ahead() {
                    self.object_literal(name, token.span)
                } else {
                    Ok(self.literal_expression(ExpressionKind::Name(name), token.span))
                }
            }
            TokenKind::LeftParen => self.parenthesized(token.span),
            TokenKind::LeftBracket => self.array_literal(token.span),
            TokenKind::LeftBrace => self.map_or_set_literal(token.span),
            TokenKind::Fn => Ok(self.function_expression(token.span, false)?),
            TokenKind::Async if self.matches(&TokenKind::LeftBrace) => {
                let block = self.block_after_open(token.span)?;
                let span = token.span.join(block.span);
                Ok(Expression {
                    kind: ExpressionKind::AsyncBlock(block),
                    span,
                })
            }
            TokenKind::Match => {
                let value = self.expression()?;
                let cases = self.match_cases()?;
                let span = token
                    .span
                    .join(cases.last().map_or(value.span, |case| case.span));
                Ok(Expression {
                    kind: ExpressionKind::Match {
                        value: Box::new(value),
                        cases,
                    },
                    span,
                })
            }
            TokenKind::New => self.new_expression(token.span),
            TokenKind::Channel => {
                let element_type = if self.matches(&TokenKind::Less) {
                    let element_type = self.type_annotation()?;
                    self.consume_type_greater("expected '>' after channel type")?;
                    Some(element_type)
                } else {
                    None
                };
                Ok(self.literal_expression(
                    ExpressionKind::Channel { element_type },
                    token.span.join(self.previous().span),
                ))
            }
            _ => Err(self.error_at(&token, "expected an expression")),
        }
    }

    fn parenthesized(&mut self, start: Span) -> Result<Expression, Diagnostic> {
        if self.matches(&TokenKind::RightParen) {
            return Ok(Expression {
                kind: ExpressionKind::Tuple(Vec::new()),
                span: start.join(self.previous().span),
            });
        }
        let first = self.expression()?;
        if !self.matches(&TokenKind::Comma) {
            self.consume(&TokenKind::RightParen, "expected ')' after expression")?;
            return Ok(first);
        }
        let mut items = vec![first];
        loop {
            items.push(self.expression()?);
            if !self.matches(&TokenKind::Comma) {
                break;
            }
            if self.check(&TokenKind::RightParen) {
                break;
            }
        }
        let end = self.consume(&TokenKind::RightParen, "expected ')' after tuple")?;
        Ok(Expression {
            kind: ExpressionKind::Tuple(items),
            span: start.join(end.span),
        })
    }

    fn array_literal(&mut self, start: Span) -> Result<Expression, Diagnostic> {
        let mut items = Vec::new();
        if !self.check(&TokenKind::RightBracket) {
            loop {
                items.push(self.expression()?);
                if !self.matches(&TokenKind::Comma) {
                    break;
                }
            }
        }
        let end = self.consume(&TokenKind::RightBracket, "expected ']' after array")?;
        Ok(Expression {
            kind: ExpressionKind::Array(items),
            span: start.join(end.span),
        })
    }

    fn map_or_set_literal(&mut self, start: Span) -> Result<Expression, Diagnostic> {
        let mut entries = Vec::new();
        let mut items = Vec::new();
        let mut is_map = self.check(&TokenKind::RightBrace);
        if !self.check(&TokenKind::RightBrace) {
            let first = self.expression()?;
            if self.matches(&TokenKind::Colon) {
                is_map = true;
                entries.push((first, self.expression()?));
                while self.matches(&TokenKind::Comma) && !self.check(&TokenKind::RightBrace) {
                    let key = self.expression()?;
                    self.consume(&TokenKind::Colon, "expected ':' between map key and value")?;
                    entries.push((key, self.expression()?));
                }
            } else {
                is_map = false;
                items.push(first);
                while self.matches(&TokenKind::Comma) && !self.check(&TokenKind::RightBrace) {
                    items.push(self.expression()?);
                }
            }
        }
        let end = self.consume(&TokenKind::RightBrace, "expected '}' after collection")?;
        Ok(Expression {
            kind: if is_map {
                ExpressionKind::Map(entries)
            } else {
                ExpressionKind::Set(items)
            },
            span: start.join(end.span),
        })
    }

    fn object_literal(&mut self, type_name: String, start: Span) -> Result<Expression, Diagnostic> {
        self.consume(&TokenKind::LeftBrace, "expected '{' for object literal")?;
        let mut fields = Vec::new();
        while !self.check(&TokenKind::RightBrace) && !self.check(&TokenKind::Eof) {
            let name = self.expect_identifier("expected object field name")?;
            self.consume(&TokenKind::Colon, "expected ':' after object field name")?;
            fields.push((name, self.expression()?));
            if !self.matches(&TokenKind::Comma) && !self.matches(&TokenKind::Semicolon) {
                break;
            }
        }
        let end = self.consume(&TokenKind::RightBrace, "expected '}' after object literal")?;
        Ok(Expression {
            kind: ExpressionKind::Object { type_name, fields },
            span: start.join(end.span),
        })
    }

    fn function_expression(
        &mut self,
        start: Span,
        is_async: bool,
    ) -> Result<Expression, Diagnostic> {
        let function = self.function(is_async, false, true)?;
        let span = start.join(function.span);
        Ok(Expression {
            kind: ExpressionKind::Function(function),
            span,
        })
    }

    fn interpolated_string(
        &self,
        parts: Vec<TokenStringPart>,
        span: Span,
    ) -> Result<Expression, Diagnostic> {
        let mut string_parts = Vec::with_capacity(parts.len());
        for part in parts {
            match part {
                TokenStringPart::Text(value) => {
                    string_parts.push(crate::ast::StringPart::Text(value));
                }
                TokenStringPart::Expression(tokens) => {
                    let expression = Parser::new(tokens).parse_expression_only()?;
                    string_parts.push(crate::ast::StringPart::Expression(expression));
                }
            }
        }
        Ok(Expression {
            kind: ExpressionKind::InterpolatedString(string_parts),
            span,
        })
    }

    fn new_expression(&mut self, start: Span) -> Result<Expression, Diagnostic> {
        let type_name = self.expect_identifier("expected type name after 'new'")?;
        self.consume(&TokenKind::LeftParen, "expected '(' after type name")?;
        let mut arguments = Vec::new();
        if !self.check(&TokenKind::RightParen) {
            loop {
                arguments.push(self.expression()?);
                if !self.matches(&TokenKind::Comma) {
                    break;
                }
            }
        }
        let end = self.consume(
            &TokenKind::RightParen,
            "expected ')' after constructor arguments",
        )?;
        Ok(Expression {
            kind: ExpressionKind::New {
                type_name,
                arguments,
            },
            span: start.join(end.span),
        })
    }

    fn literal_expression(&self, kind: ExpressionKind, span: Span) -> Expression {
        Expression { kind, span }
    }

    fn expect_identifier(&mut self, message: &str) -> Result<String, Diagnostic> {
        let token = self.advance().clone();
        match token.kind {
            TokenKind::Identifier(name) => Ok(name),
            _ => Err(self.error_at(&token, message)),
        }
    }

    fn consume_optional_semicolon(&mut self) -> Span {
        let _ = self.matches(&TokenKind::Semicolon);
        self.previous().span
    }

    fn is_statement_end(&self) -> bool {
        self.check(&TokenKind::Semicolon)
            || self.check(&TokenKind::RightBrace)
            || self.check(&TokenKind::Eof)
    }

    fn consume(&mut self, kind: &TokenKind, message: &str) -> Result<&Token, Diagnostic> {
        if self.check(kind) {
            Ok(self.advance())
        } else {
            let token = self.peek().clone();
            Err(self.error_at(&token, message))
        }
    }

    fn consume_type_greater(&mut self, message: &str) -> Result<(), Diagnostic> {
        if self.matches(&TokenKind::Greater) {
            return Ok(());
        }
        if self.check(&TokenKind::ShiftRight) {
            let token = self.advance().clone();
            let remainder = Token {
                kind: TokenKind::Greater,
                span: Span::new(token.span.start + 1, token.span.end),
            };
            self.tokens.insert(self.current, remainder);
            return Ok(());
        }
        let token = self.peek().clone();
        Err(self.error_at(&token, message))
    }

    fn is_object_literal_ahead(&self) -> bool {
        self.check(&TokenKind::LeftBrace)
            && self.check_at(1, &TokenKind::Identifier(String::new()))
            && self.check_at(2, &TokenKind::Colon)
    }

    fn matches(&mut self, kind: &TokenKind) -> bool {
        if self.check(kind) {
            self.advance();
            true
        } else {
            false
        }
    }

    fn check_at(&self, distance: usize, kind: &TokenKind) -> bool {
        self.tokens
            .get(self.current + distance)
            .is_some_and(|token| {
                std::mem::discriminant(&token.kind) == std::mem::discriminant(kind)
            })
    }

    fn check(&self, kind: &TokenKind) -> bool {
        self.check_at(0, kind)
    }

    fn advance(&mut self) -> &Token {
        if !self.check(&TokenKind::Eof) {
            self.current += 1;
        }
        self.previous()
    }

    fn peek(&self) -> &Token {
        &self.tokens[self.current]
    }

    fn previous(&self) -> &Token {
        &self.tokens[self.current - 1]
    }

    fn error_at(&self, token: &Token, message: &str) -> Diagnostic {
        Diagnostic::error("NXL1001", message, token.span)
    }
}

fn statement_span(statement: &Statement) -> Span {
    match statement {
        Statement::If { span, .. }
        | Statement::For { span, .. }
        | Statement::While { span, .. }
        | Statement::Loop { span, .. }
        | Statement::Match { span, .. }
        | Statement::Try { span, .. }
        | Statement::Throw { span, .. }
        | Statement::Module { span, .. }
        | Statement::Import { span, .. } => *span,
        Statement::Let { span, .. } | Statement::Return { span, .. } => *span,
        Statement::Break(span) | Statement::Continue(span) => *span,
        Statement::Function(function) => function.span,
        Statement::TypeDeclaration(declaration) => declaration.span,
        Statement::TypeAlias(alias) => alias.span,
        Statement::ImplBlock(implementation) => implementation.span,
        Statement::Expression(expression) => expression.span,
        Statement::Block(block) | Statement::Unsafe(block) => block.span,
        Statement::Yield { span, .. } => *span,
        Statement::Export(inner) => statement_span(inner),
    }
}
