use oxc_allocator::{Allocator, CloneIn};
use oxc_ast::{
    ast::{
        BindingRestElement, Expression, FormalParameterKind, FunctionType, JSXChild, JSXElement,
        JSXElementName, JSXText, Program, PropertyKind, Statement, TSThisParameter,
        TSTypeAnnotation, TSTypeParameterDeclaration, TSTypeParameterInstantiation,
        VariableDeclarationKind,
    },
    AstBuilder,
};
use oxc_codegen::{Codegen, CodegenOptions, Context, Gen};
use oxc_parser::{Parser, ParserReturn};
use oxc_semantic::{SemanticBuilder, SymbolTable};
use oxc_span::{SourceType, SPAN};
use oxc_traverse::{traverse_mut, Traverse, TraverseCtx};

#[allow(unused)]
enum MaybeRef<'a, T> {
    Owned(T),
    Borrowed(&'a T),
}

#[allow(unused)]
enum Prop<'a> {
    Static(&'a str, &'a str),
    Dynamic(&'a str, &'a Expression<'a>),
}

#[allow(unused)]
enum Child<'a> {
    Text(String),
    Element(Element<'a>),
    Expression(&'a Expression<'a>),
}

#[allow(unused, clippy::upper_case_acronyms)]
enum Kind {
    DOM,
    SVG,
    Custom,
    Component,
}

#[allow(unused)]
struct Element<'a> {
    name: String,
    kind: Kind,
    props: Vec<Prop<'a>>,
    children: Vec<Child<'a>>,

    ast_builder: &'a AstBuilder<'a>,
}

#[allow(unused)]
impl<'a> Element<'a> {
    fn from_jsx(ast_builder: &'a AstBuilder, node: &'a JSXElement<'a>) -> Self {
        let (name, kind) = match &node.opening_element.name {
            JSXElementName::Identifier(id) => (id.name.to_string(), Kind::DOM),
            JSXElementName::ThisExpression(_) => ("this".to_string(), Kind::Component),
            JSXElementName::NamespacedName(id) => (
                format!("{}:{}", id.namespace.name, id.property.name),
                Kind::Component,
            ),
            JSXElementName::MemberExpression(_) => ("TODO".to_string(), Kind::Component),
            JSXElementName::IdentifierReference(id) => (id.name.to_string(), Kind::Component),
        };

        Element {
            ast_builder,
            name,
            kind,
            props: vec![],
            children: vec![],
        }
    }

    fn js(&'a self) -> Expression<'a> {
        struct Builder<'a> {
            templates: Vec<String>,
            inserts: Vec<(usize, usize, MaybeRef<'a, Expression<'a>>)>,
            ast_builder: &'a AstBuilder<'a>,
        }

        impl<'a> Builder<'a> {
            fn build(element: &'a Element<'a>, ast_builder: &'a AstBuilder<'a>) -> Self {
                let mut result = Builder {
                    templates: vec![String::new()],
                    inserts: vec![],
                    ast_builder,
                };
                result.descend(element, 0, 0);
                result
            }

            fn descend(&mut self, element: &'a Element, x: usize, y: usize) {
                if let Kind::DOM = element.kind {
                    let static_props = element.format_props();
                    self.templates[y]
                        .push_str(format!("<{}{}>", element.name, static_props).as_str());
                }
                for (i, child) in element.children.iter().enumerate() {
                    let x = x + i + 1;
                    match child {
                        Child::Text(text) => {
                            self.templates[y].push_str(text);
                        }
                        Child::Element(child_element) => {
                            if let Kind::Component = child_element.kind {
                                self.templates.push(String::new());
                                self.descend(child_element, 0, y + 1);
                                if let Some(call) = child_element.call_with_props() {
                                    self.inserts.push((x, y, MaybeRef::Owned(call)));
                                }
                            } else {
                                self.descend(child_element, x, y);
                            }
                        }
                        Child::Expression(expr) => {
                            self.inserts.push((x, y, MaybeRef::Borrowed(expr)));
                        }
                    }
                }
            }
        }

        let mut statements = self.ast_builder.vec();
        let Builder {
            templates, inserts, ..
        } = Builder::build(self, self.ast_builder);
        let templates_declarations = self.templates_declaration(templates);
        statements.push(templates_declarations);
        for (x, y, expr) in inserts {
            let expr = match &expr {
                MaybeRef::Borrowed(expr) => expr,
                MaybeRef::Owned(expr) => expr,
            };
            let callee = self
                .ast_builder
                .expression_identifier_reference(SPAN, "$insert");
            let after = self
                .ast_builder
                .expression_identifier_reference(SPAN, format!("$el{y}{x}"));
            let after = self.ast_builder.argument_expression(after);
            let mut args = self.ast_builder.vec1(after);
            let arg = self
                .ast_builder
                .argument_expression(expr.clone_in(self.ast_builder.allocator));
            args.push(arg);
            let call = self.ast_builder.expression_call(
                SPAN,
                callee,
                Option::<TSTypeParameterInstantiation>::None,
                args,
                false,
            );
            statements.push(self.ast_builder.statement_expression(SPAN, call));
        }
        let call = self.wrap_in_function_call(statements);
        call
    }

    fn call_with_props(&self) -> Option<Expression<'a>> {
        if let Kind::Component = self.kind {
            let mut props = self.ast_builder.vec();
            for prop in &self.props {
                match prop {
                    Prop::Static(name, value) => {
                        let key = if name.contains("-") {
                            self.ast_builder.property_key_expression(
                                self.ast_builder
                                    .expression_string_literal(SPAN, self.ast_builder.atom(name)),
                            )
                        } else {
                            self.ast_builder
                                .property_key_identifier_name(SPAN, self.ast_builder.atom(name))
                        };
                        let value = self
                            .ast_builder
                            .expression_string_literal(SPAN, self.ast_builder.atom(value));
                        let prop = self.ast_builder.object_property_kind_object_property(
                            SPAN,
                            PropertyKind::Init,
                            key,
                            value,
                            None,
                            false,
                            false,
                            false,
                        );
                        props.push(prop);
                    }
                    Prop::Dynamic(name, value) => {
                        let key = if name.contains("-") {
                            self.ast_builder.property_key_expression(
                                self.ast_builder
                                    .expression_string_literal(SPAN, self.ast_builder.atom(name)),
                            )
                        } else {
                            self.ast_builder
                                .property_key_identifier_name(SPAN, self.ast_builder.atom(name))
                        };
                        let prop = self.ast_builder.object_property_kind_object_property(
                            SPAN,
                            PropertyKind::Init,
                            key,
                            value.clone_in(self.ast_builder.allocator),
                            None,
                            false,
                            false,
                            false,
                        );
                        props.push(prop);
                    }
                }
            }
            let props = self.ast_builder.expression_object(SPAN, props, None);
            let args = self
                .ast_builder
                .vec1(self.ast_builder.argument_expression(props));
            let callee = self
                .ast_builder
                .expression_identifier_reference(SPAN, self.ast_builder.atom(self.name.as_str()));
            let node = self.ast_builder.expression_call(
                SPAN,
                callee,
                Option::<TSTypeParameterInstantiation>::None,
                args,
                false,
            );
            return Some(node);
        }
        None
    }

    fn format_props(&self) -> String {
        let mut static_props = String::new();
        for prop in &self.props {
            match prop {
                Prop::Static(name, value) => {
                    static_props.push_str(format!(" {}=\"{}\"", name, value).as_str());
                }
                Prop::Dynamic(_name, _value) => {}
            }
        }
        static_props
    }

    fn templates_declaration(&self, templates_strings: Vec<String>) -> Statement<'a> {
        let mut declarations = self.ast_builder.vec();
        for (i, template) in templates_strings.iter().enumerate() {
            let id = self
                .ast_builder
                .binding_pattern_kind_binding_identifier(SPAN, format!("$template{}", i));
            let id = self
                .ast_builder
                .binding_pattern(id, Option::<TSTypeAnnotation>::None, false);
            let callee = self
                .ast_builder
                .expression_identifier_reference(SPAN, "$template");
            let arg = self.ast_builder.expression_string_literal(SPAN, template);
            let arg = self.ast_builder.argument_expression(arg);
            let args = self.ast_builder.vec1(arg);
            let init = self.ast_builder.expression_call(
                SPAN,
                callee,
                Option::<TSTypeParameterInstantiation>::None,
                args,
                false,
            );
            let declaration = self.ast_builder.variable_declarator(
                SPAN,
                VariableDeclarationKind::Const,
                id,
                Some(init),
                false,
            );
            declarations.push(declaration);
        }
        let declarations = self.ast_builder.declaration_variable(
            SPAN,
            VariableDeclarationKind::Const,
            declarations,
            false,
        );
        let declaration = self.ast_builder.statement_declaration(declarations);
        declaration
    }

    fn wrap_in_function_call(
        &self,
        statements: oxc_allocator::Vec<'a, Statement<'a>>,
    ) -> Expression<'a> {
        let body = self
            .ast_builder
            .alloc_function_body(SPAN, self.ast_builder.vec(), statements);
        let params = self.ast_builder.alloc_formal_parameters(
            SPAN,
            FormalParameterKind::FormalParameter,
            self.ast_builder.vec(),
            Option::<BindingRestElement>::None,
        );
        let callee = self.ast_builder.expression_function(
            FunctionType::FunctionExpression,
            SPAN,
            None,
            false,
            false,
            false,
            Option::<TSTypeParameterDeclaration>::None,
            Option::<TSThisParameter>::None,
            params,
            Option::<TSTypeAnnotation>::None,
            Some(body),
        );
        let call = self.ast_builder.expression_call(
            SPAN,
            callee,
            Option::<TSTypeParameterInstantiation>::None,
            self.ast_builder.vec(),
            false,
        );
        call
    }
}

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
    ast_builder: AstBuilder<'a>,
    templates: Vec<String>,
    chunks: Vec<String>,
    slices: Vec<usize>,
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
            ast_builder,
            program,
            templates: Vec::new(),
            chunks: Vec::new(),
            slices: Vec::new(),
            positions: Vec::new(),
        }
    }

    pub fn add(&mut self, src: &'a str) {
        let allocator = self.ast_builder.allocator;
        let source_type = self.program.source_type;
        let parser = Parser::new(allocator, src, source_type);
        let ParserReturn {
            mut program,
            errors: _errors,
            trivias: _trivias,
            panicked: _panicked,
        } = parser.parse();

        let (symbols, scopes) = SemanticBuilder::new(src)
            .build(&program)
            .semantic
            .into_symbol_table_and_scope_tree();

        let (_symbols, _scopes) = traverse_mut(self, allocator, &mut program, symbols, scopes);

        for statement in program.body {
            self.program.body.push(statement);
        }
    }

    pub fn js(&self) -> String {
        let codegen = Codegen::new().with_options(CodegenOptions {
            single_quote: true,
            minify: false,
        });
        let result = codegen.build(&self.program);
        result.source_text
    }
}

#[allow(clippy::single_match)]
impl<'a> Traverse<'a> for Bundle<'a> {
    fn enter_jsx_element(&mut self, node: &mut JSXElement<'a>, _ctx: &mut TraverseCtx<'a>) {
        let (name, is_component) = match &node.opening_element.name {
            JSXElementName::Identifier(id) => (id.to_string(), false),
            JSXElementName::IdentifierReference(id) => (id.to_string(), true),
            JSXElementName::MemberExpression(id) => (id.to_string(), true),
            JSXElementName::NamespacedName(id) => (id.to_string(), true),
            _ => {
                return;
            }
        };
        if !is_component {
            if let Some(last) = self.slices.last_mut() {
                *last += 1;
            }
            if let Some(last) = self.positions.last_mut() {
                last.push(0);
            }
            self.chunks.push(format!("<{name}>"));
        } else {
            if let Some(last) = self.slices.last_mut() {
                *last += 1;
            }
            if let Some(last) = self.positions.last_mut() {
                if let Some(last) = last.last_mut() {
                    *last += 1;
                }
            }
            self.chunks.push("<!>".to_string());
            self.slices.push(0);
            self.positions.push(vec![]);
        }
    }

    fn exit_jsx_element(&mut self, node: &mut JSXElement<'a>, _ctx: &mut TraverseCtx<'a>) {
        let (name, is_component) = match &node.opening_element.name {
            JSXElementName::Identifier(id) => (id.to_string(), false),
            JSXElementName::IdentifierReference(id) => (id.to_string(), true),
            JSXElementName::MemberExpression(id) => (id.to_string(), true),
            JSXElementName::NamespacedName(id) => (id.to_string(), true),
            _ => {
                return;
            }
        };
        if !is_component {
            if let Some(last) = self.positions.last_mut() {
                last.pop();
                if let Some(last) = last.last_mut() {
                    *last += 1;
                }
            }
            if let Some(last) = self.slices.last_mut() {
                *last += 1;
            }
            self.chunks.push(format!("</{name}>"));
        } else {
            self.positions.pop();
            let id = self.slices.len() - 1;
            let Some(len) = self.slices.pop() else {
                return;
            };
            if let Some(position) = self.positions.last() {
                let mut codegen = Codegen::new();
                node.gen(&mut codegen, Context::default());
                println!(
                    "Insert into {:?} component #{} {:?}",
                    position,
                    id,
                    codegen.into_source_text()
                );
            }
            let len = self.chunks.len() - len;
            let template = self.chunks.drain(len..).collect::<Vec<_>>().join("");
            println!("{:?}", template);
            self.templates.push(template);
        }
    }

    fn enter_jsx_text(&mut self, node: &mut JSXText<'a>, _ctx: &mut TraverseCtx<'a>) {
        let text = trim_jsx_text(node);
        if !text.is_empty() {
            self.chunks.push(text);
            if let Some(last) = self.slices.last_mut() {
                *last += 1;
            }
            if let Some(last) = self.positions.last_mut() {
                if let Some(last) = last.last_mut() {
                    *last += 1;
                }
            }
        }
    }

    fn enter_jsx_expression_container(
        &mut self,
        _node: &mut oxc_ast::ast::JSXExpressionContainer<'a>,
        _ctx: &mut TraverseCtx<'a>,
    ) {
        self.chunks.push("<!>".to_string());
        if let Some(last) = self.slices.last_mut() {
            *last += 1;
        }
    }

    fn exit_jsx_expression_container(
        &mut self,
        node: &mut oxc_ast::ast::JSXExpressionContainer<'a>,
        _ctx: &mut TraverseCtx<'a>,
    ) {
        if let Some(last) = self.positions.last_mut() {
            let mut codegen = Codegen::new();
            node.expression.gen(&mut codegen, Context::default());
            println!("Insert into {:?} {:?}", last, codegen.into_source_text());
            if let Some(last) = last.last_mut() {
                *last += 1;
            }
        }
    }

    fn enter_jsx_child(
        &mut self,
        node: &mut oxc_ast::ast::JSXChild<'a>,
        _ctx: &mut TraverseCtx<'a>,
    ) {
        match node {
            JSXChild::Element(_node) => {}
            JSXChild::Text(_node) => {}
            JSXChild::Spread(_node) => {}
            JSXChild::Fragment(_node) => {}
            JSXChild::ExpressionContainer(_node) => {}
        };
    }

    fn enter_expression(&mut self, node: &mut Expression<'a>, _ctx: &mut TraverseCtx<'a>) {
        match node {
            Expression::JSXElement(_node) => {
                self.slices.push(0);
                self.positions.push(vec![]);
            }
            _ => {}
        };
    }

    fn exit_expression(&mut self, node: &mut Expression, _ctx: &mut TraverseCtx) {
        match node {
            Expression::JSXElement(_node) => {
                self.positions.pop();
                let len = self.slices.pop().unwrap_or_default();
                let len = self.chunks.len() - len;
                let template = self.chunks.drain(len..).collect::<Vec<_>>().join("");
                println!("{:?}", template);
            }
            _ => {}
        };
    }
}
