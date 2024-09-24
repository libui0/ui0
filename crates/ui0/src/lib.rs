use std::path::Path;

use oxc_allocator::{Allocator, IntoIn};
use oxc_ast::{
    ast::{
        Declaration, ExportDefaultDeclarationKind, Expression, JSXChild, JSXElement,
        JSXElementName, JSXExpressionContainer, JSXText, ModuleDeclaration, NumberBase, Program,
        Statement, TSTypeParameterInstantiation,
    },
    AstBuilder,
};
use oxc_codegen::{Codegen, CodegenOptions};
use oxc_parser::{Parser, ParserReturn};
use oxc_semantic::{SemanticBuilder, SymbolTable};
use oxc_span::{SourceType, SPAN};
use oxc_transformer::{TransformOptions, Transformer, TypeScriptOptions};
use oxc_traverse::{traverse_mut, Traverse, TraverseCtx};

fn trim_jsx_text(node: &JSXText) -> String {
    let mut buf = String::new();
    let text = node.value.replace('\t', " ");
    let lines = text.lines();
    let count = lines.clone().count();
    for (i, line) in lines.enumerate().filter(|(_, line)| !line.is_empty()) {
        let mut line = line;
        if i != 0 {
            line = line.trim_start_matches(' ')
        }
        if i != count - 1 {
            line = line.trim_end_matches(' ')
        }
        if line.is_empty() {
            continue;
        }
        if i != 0 && !buf.is_empty() {
            buf.push(' ')
        }
        buf.push_str(line);
    }
    buf
}

pub struct Bundle<'a> {
    program: Program<'a>,
    ast: AstBuilder<'a>,
    inserts: Vec<Vec<Expression<'a>>>,
    templates: Vec<Vec<String>>,
    positions: Vec<Vec<usize>>,
}

impl<'a> Bundle<'a> {
    pub fn new(allocator: &'a Allocator) -> Self {
        let ast_builder = AstBuilder::new(allocator);
        let source_type = SourceType::default()
            .with_jsx(true)
            .with_typescript(true)
            .with_module(true);
        let program = ast_builder.program(
            SPAN,
            source_type,
            None,
            ast_builder.vec(),
            ast_builder.vec(),
        );
        let _symbols = SymbolTable::default();
        Bundle {
            ast: ast_builder,
            program,
            templates: Vec::new(),
            positions: Vec::new(),
            inserts: Vec::new(),
        }
    }

    pub fn add(&mut self, src: &'a str) {
        let allocator = self.ast.allocator;
        let source_type = self.program.source_type;
        let parser = Parser::new(allocator, src, source_type);
        let ParserReturn {
            mut program,
            errors: _errors,
            trivias,
            panicked: _panicked,
        } = parser.parse();

        let (symbols, scopes) = SemanticBuilder::new(src)
            .build(&program)
            .semantic
            .into_symbol_table_and_scope_tree();

        let (symbols, scopes) = traverse_mut(self, allocator, &mut program, symbols, scopes);
        let (_symbols, _scopes) = traverse_mut(
            &mut Transformer::new(
                allocator,
                Path::new(""),
                source_type,
                src,
                trivias,
                TransformOptions {
                    typescript: TypeScriptOptions::default(),
                    ..Default::default()
                },
            ),
            allocator,
            &mut program,
            symbols,
            scopes,
        );

        for statement in program.body {
            self.program.body.push(statement);
        }
    }

    pub fn js(&self) -> String {
        let codegen = Codegen::new().with_options(CodegenOptions {
            single_quote: true,
            minify: false,
        });
        let src = codegen.build(&self.program);
        src.source_text
    }

    fn enter_element_dom(&mut self, name: &str) {
        self.positions.push(vec![0]);
        self.inserts.push(vec![]);
        self.templates.push(vec![format!("<{name}>")]);
    }

    fn exit_element_dom(&mut self, name: &str, node: &mut Expression<'a>) {
        self.positions.pop();
        self.templates
            .last_mut()
            .unwrap()
            .push(format!("</{name}>"));
        let template = self.templates.pop().unwrap().join("");
        let template = self.ast.expression_string_literal(SPAN, template);
        let mut args = self.ast.vec1(self.ast.argument_expression(template));
        for insert in self.inserts.pop().unwrap() {
            args.push(self.ast.argument_expression(insert));
        }

        let callee = self.ast.expression_identifier_reference(SPAN, "$render");
        let call = self.ast.expression_call(
            SPAN,
            callee,
            Option::<TSTypeParameterInstantiation>::None,
            args,
            false,
        );
        *node = call;
    }

    fn enter_child_dom(&mut self, name: &str) {
        self.positions.last_mut().unwrap().push(0);
        self.templates.last_mut().unwrap().push(format!("<{name}>"));
    }

    fn exit_child_dom(&mut self, name: &str) {
        self.positions.last_mut().unwrap().pop();
        *self.positions.last_mut().unwrap().last_mut().unwrap() += 1;
        self.templates
            .last_mut()
            .unwrap()
            .push(format!("</{name}>"));
    }

    fn enter_child_component(&mut self) {
        self.inserts.push(vec![]);
        self.templates.push(vec![]);
        self.positions.push(vec![0]);
    }

    fn exit_child_component(&mut self, name: &str, _node: &mut JSXElement<'a>) {
        let _template = self.templates.pop().unwrap();
        let _inserts = self.inserts.pop();
        self.positions.pop();
        self.templates.last_mut().unwrap().push("<!>".to_string());

        let position = self.positions.last().unwrap();
        let callee = self.ast.expression_identifier_reference(SPAN, name);
        let call = self.ast.expression_call(
            SPAN,
            callee,
            Option::<TSTypeParameterInstantiation>::None,
            self.ast.vec(),
            false,
        );
        let mut path = self.ast.vec();
        for number in position {
            let number = self.ast.expression_numeric_literal(
                SPAN,
                0.0,
                number.to_string(),
                NumberBase::Decimal,
            );
            let number = self.ast.array_expression_element_expression(number);
            path.push(number);
        }
        let path = self.ast.expression_array(SPAN, path, None);
        let mut args = self.ast.vec1(self.ast.argument_expression(path));
        args.push(self.ast.argument_expression(call));
        let callee = self.ast.expression_identifier_reference(SPAN, "$insert");
        let call = self.ast.expression_call(
            SPAN,
            callee,
            Option::<TSTypeParameterInstantiation>::None,
            args,
            false,
        );

        self.inserts.last_mut().unwrap().push(call);
        *self.positions.last_mut().unwrap().last_mut().unwrap() += 1;
    }

    fn exit_child_expression(&mut self, node: &mut JSXExpressionContainer<'a>) {
        let Some(expression) = node.expression.as_expression_mut() else {
            return;
        };
        self.templates.last_mut().unwrap().push("<!>".to_string());
        let position = self.positions.last().unwrap();
        let mut path = self.ast.vec();
        for number in position {
            let number = self.ast.expression_numeric_literal(
                SPAN,
                0.0,
                number.to_string(),
                NumberBase::Decimal,
            );
            let number = self.ast.array_expression_element_expression(number);
            path.push(number);
        }
        let path = self.ast.expression_array(SPAN, path, None);
        let mut args = self.ast.vec1(self.ast.argument_expression(path));
        args.push(
            self.ast
                .argument_expression(self.ast.move_expression(expression)),
        );
        let callee = self.ast.expression_identifier_reference(SPAN, "$insert");
        let call = self.ast.expression_call(
            SPAN,
            callee,
            Option::<TSTypeParameterInstantiation>::None,
            args,
            false,
        );
        self.inserts.last_mut().unwrap().push(call);
        *self.positions.last_mut().unwrap().last_mut().unwrap() += 1;
    }

    fn exit_child_text(&mut self, node: &mut JSXText<'a>) {
        let text = trim_jsx_text(node);
        if !text.is_empty() {
            *self.positions.last_mut().unwrap().last_mut().unwrap() += 1;
            self.templates.last_mut().unwrap().push(text);
        }
    }

    fn get_element_name_and_type(&self, node: &JSXElement<'a>) -> Option<(String, bool)> {
        match &node.opening_element.name {
            JSXElementName::Identifier(id) => Some((id.to_string(), false)),
            JSXElementName::IdentifierReference(id) => Some((id.to_string(), true)),
            JSXElementName::MemberExpression(id) => Some((id.to_string(), true)),
            JSXElementName::NamespacedName(id) => Some((id.to_string(), true)),
            _ => None,
        }
    }
}

#[allow(clippy::single_match)]
impl<'a> Traverse<'a> for Bundle<'a> {
    fn enter_jsx_child(&mut self, node: &mut JSXChild<'a>, _ctx: &mut TraverseCtx<'a>) {
        match node {
            JSXChild::Element(node) => {
                let Some((name, is_component)) = self.get_element_name_and_type(node) else {
                    return;
                };
                if !is_component {
                    self.enter_child_dom(&name);
                } else {
                    self.enter_child_component();
                }
            }
            _ => {}
        };
    }

    fn exit_jsx_child(&mut self, node: &mut JSXChild<'a>, _ctx: &mut TraverseCtx<'a>) {
        match node {
            JSXChild::Element(node) => {
                let Some((name, is_component)) = self.get_element_name_and_type(node) else {
                    return;
                };
                if !is_component {
                    self.exit_child_dom(&name);
                } else {
                    self.exit_child_component(&name, node);
                }
            }
            JSXChild::Text(node) => {
                self.exit_child_text(node);
            }
            JSXChild::Spread(_node) => {}
            JSXChild::Fragment(_node) => {}
            JSXChild::ExpressionContainer(node) => {
                self.exit_child_expression(node);
            }
        };
    }

    fn enter_expression(&mut self, expression: &mut Expression<'a>, _ctx: &mut TraverseCtx<'a>) {
        match expression {
            Expression::JSXElement(node) => {
                let Some((name, is_component)) = self.get_element_name_and_type(node) else {
                    return;
                };
                if !is_component {
                    self.enter_element_dom(&name);
                }
            }
            _ => {}
        };
    }

    fn exit_expression(&mut self, expression: &mut Expression<'a>, _ctx: &mut TraverseCtx) {
        match expression {
            Expression::JSXElement(node) => {
                let Some((name, is_component)) = self.get_element_name_and_type(node) else {
                    return;
                };
                if !is_component {
                    self.exit_element_dom(&name, expression);
                }
            }
            _ => {}
        };
    }

    fn exit_program(&mut self, program: &mut Program<'a>, _ctx: &mut TraverseCtx<'a>) {
        for statement in program.body.iter_mut() {
            if let Statement::ExportDefaultDeclaration(declaration) = statement {
                if let ExportDefaultDeclarationKind::FunctionDeclaration(_) = &declaration.declaration {
                    let Statement::ExportDefaultDeclaration(declaration) = self.ast.move_statement(statement) else { continue; };
                    let ExportDefaultDeclarationKind::FunctionDeclaration(function) = declaration.unbox().declaration else { continue; };
                    let declaration = self.ast.declaration_from_function(function.unbox());
                    *statement = self.ast.statement_declaration(declaration);
                }
            }

        }
    }
}
