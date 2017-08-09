use ::ast::*;
use ::module::Module;
use ::type_system::error::{ TypeError, TypeCheckResult };
use ::type_system::symbol_table::SymbolTable;
use ::type_system::type_environment::{ TypeEnvironment, TypeReference };

pub fn type_check(type_environment: &mut TypeEnvironment, symbol_table: &mut SymbolTable, module: &mut Module) -> TypeCheckResult<()> {
    symbol_table.enter_scope();
    try!(check_primitives(module.is_core(), type_environment, symbol_table, &mut module.find_primitives_mut()));
    try!(check_structs(type_environment, symbol_table, &mut module.find_structs_mut()));
    try!(check_casts(module.is_core(), type_environment, symbol_table, &mut module.find_casts_mut()));
    try!(check_constant(symbol_table, &mut module.find_constants_mut()));
    try!(check_functions(type_environment, symbol_table, &mut module.find_functions_mut()));
    symbol_table.leave_scope();
    Ok(())
}

fn check_primitives(is_core_module: bool, type_environment: &mut TypeEnvironment, symbol_table: &mut SymbolTable, primitives: &mut Vec<&mut PrimitiveDeclaration>) -> TypeCheckResult<()> {
    if !is_core_module && primitives.len() > 0 {
        return Err(TypeError::SyntaxOnlyValidInCoreModule)
    }
    for p in primitives.iter_mut() {
        let reference = try!(type_environment.create_type(&p.type_name.name));
        try!(symbol_table.add_global_type(&p.type_name.name, reference));
        p.declaring_type = Some(reference);
    }
    Ok(())
}

fn check_structs(type_environment: &mut TypeEnvironment, symbol_table: &mut SymbolTable, structs: &mut Vec<&mut StructDefinition>) -> TypeCheckResult<()> {
    for s in structs.iter_mut() {
        try!(symbol_table.add_symbol(&s.struct_name.name));
        let reference = try!(type_environment.create_type(&s.struct_name.name));
        try!(symbol_table.add_type(&s.struct_name.name, reference));
        s.declaring_type = Some(reference);
    }

    for s in structs.iter_mut() {
        for member in s.struct_member.iter_mut() {
            let struct_member_type = try!(symbol_table.find_type_or_err(&member.struct_member_type_name.name));
            member.struct_member_type = Some(struct_member_type);
        }
    }
    
    Ok(())
}

fn check_casts(is_core_module: bool, type_environment: &mut TypeEnvironment, symbol_table: &mut SymbolTable, casts: &mut Vec<&mut CastDeclaration>) -> TypeCheckResult<()> {
    if !is_core_module && casts.len() > 0 {
        return Err(TypeError::SyntaxOnlyValidInCoreModule)
    }
    for c in casts.iter_mut() {
        let source_type = try!(symbol_table.find_type_or_err(&c.source_type.name));
        let target_type = try!(symbol_table.find_type_or_err(&c.target_type.name));

        let mut source_type = match type_environment.find_type_mut(source_type.clone()) {
            Some(t) => t,
            None => return Err(TypeError::TypeNotFound(c.source_type.name.clone()))
        };

        if source_type.does_cast_exist(target_type.clone()) {
            return Err(TypeError::CastAlreadyDeclared(c.source_type.name.clone(), c.target_type.name.clone()));
        }

        match c.cast_type {
            CastType::Implicit => source_type.add_implicit_cast(&target_type),
            CastType::Explicit => source_type.add_explicit_cast(&target_type),
        }
    }
    Ok(())
}

fn check_constant(symbol_table: &mut SymbolTable, constants: &mut Vec<&mut ConstantDefinition>) -> TypeCheckResult<()> {
    for s in constants.iter_mut() {
        try!(symbol_table.add_symbol(&s.constant_name.name));

        let type_ref = try!(symbol_table.find_type_or_err(&s.constant_type_name.name));
        
        s.constant_type = Some(type_ref.clone());
        try!(symbol_table.resolve_symbol_type(&s.constant_name.name, type_ref.clone()))
    }
    Ok(())
}

fn check_functions(type_environment: &mut TypeEnvironment, symbol_table: &mut SymbolTable, functions: &mut Vec<&mut FunctionDeclaration>) -> TypeCheckResult<()> {
    for f in functions.iter_mut() {
        let function_type = try!(type_environment.create_type(&f.function_name.name));
        try!(symbol_table.add_symbol_with_type(&f.function_name.name, function_type));
        symbol_table.enter_scope();
        for argument in f.arguments.iter_mut() {
            let type_ref = try!(symbol_table.find_type_or_err(&argument.argument_type_name.name));
            argument.argument_type = Some(type_ref.clone());
            try!(symbol_table.add_symbol_with_type(&argument.argument_name.name, type_ref.clone()));
        }

        let type_ref = try!(symbol_table.find_type_or_err(&f.return_type_name.name));
        f.return_type = Some(type_ref.clone());

        try!(check_block(symbol_table, &mut f.block));
        symbol_table.leave_scope();
    }
    Ok(())
}

fn check_block(symbol_table: &mut SymbolTable, block: &mut BlockDeclaration) -> TypeCheckResult<()> {
    symbol_table.enter_scope();
    for s in block.statements.iter_mut() {
        match *s {
            BlockStatement::Local(ref mut local_declaration) => {
                let local_type = try!(check_expression(symbol_table, &mut local_declaration.expression));
                local_declaration.local_type = Some(local_type.clone());
                try!(symbol_table.add_symbol_with_type(&local_declaration.symbol_name.name, local_type));
            },
            BlockStatement::Return(ref mut return_declaration) => {
                let return_type = try!(check_expression(symbol_table, &mut return_declaration.expression));
                return_declaration.return_type = Some(return_type);
            },
            BlockStatement::Expression(ref mut expression_statement) => {
                try!(check_expression(symbol_table, expression_statement));
            }
        }
    }
    symbol_table.leave_scope();
    Ok(())
}

fn check_expression(symbol_table: &mut SymbolTable, expression: &mut ExpressionStatement) -> TypeCheckResult<TypeReference> {
    match *expression {
        ExpressionStatement::Literal(ref mut literal_expression) => check_literal_expression(symbol_table, literal_expression),
        ExpressionStatement::Variable(ref mut variable_expression) => check_variable_expression(symbol_table, variable_expression),
        ExpressionStatement::Infix(ref mut infix_expression) => check_infix_expression(symbol_table, infix_expression),
        ExpressionStatement::StructInstantiation(ref mut struct_instantiation_expression) => check_struct_instatiation_expression(symbol_table, struct_instantiation_expression),
        _ => Ok(TypeReference::new(0)), // TODO implement remaining expressions
    }
}

fn check_struct_instatiation_expression(symbol_table: &mut SymbolTable, struct_instantiation_expression: &mut StructInstantiationExpression) -> TypeCheckResult<TypeReference> {
    let struct_type = try!(symbol_table.find_type_or_err(&struct_instantiation_expression.struct_type_name.name));
    struct_instantiation_expression.struct_type = Some(struct_type);

    for initializer in struct_instantiation_expression.struct_field_initializer.iter_mut() {
        // TODO check if initializer is same as or convertible to struct field type
        let initializer_type = try!(check_expression(symbol_table, &mut *initializer.initializer));
        initializer.struct_field_type = Some(initializer_type);
    }

    Ok(struct_type)
}

fn check_infix_expression(symbol_table: &mut SymbolTable, infix_expression: &mut InfixExpression) -> TypeCheckResult<TypeReference> {
    let left_hand_type = try!(check_expression(symbol_table, &mut *infix_expression.left_hand));
    let right_hand_type = try!(check_expression(symbol_table, &mut *infix_expression.right_hand));

    // TODO associativity

    if left_hand_type != right_hand_type {
        // TODO implicit casts
        // TODO uncomment
        // return Err(TypeError::IncompatibleTypes("".to_string(), "".to_string())); // TODO refactor type_environment and symbol_table
    }

    // TODO check if operator is available

    infix_expression.infix_type = Some(left_hand_type);

    Ok(left_hand_type)
}

fn check_variable_expression(symbol_table: &mut SymbolTable, variable_expression: &mut VariableExpression) -> TypeCheckResult<TypeReference> {
    match symbol_table.find_symbol(&variable_expression.variable_name.name) {
        Some(ref symbol) => match symbol.get_type() {
            Some(t) => {
                variable_expression.variable_type = Some(t.clone());
                return Ok(t);
            },
            None => return Err(TypeError::CannotInfer(variable_expression.variable_name.name.clone())),
        },
        None => return Err(TypeError::VariableNotFound(variable_expression.variable_name.name.clone())),
    }
}

fn check_literal_expression(symbol_table: &mut SymbolTable, literal_expression: &mut LiteralExpression) -> TypeCheckResult<TypeReference> {
    let type_ref = match literal_expression.literal_expression_type {
        LiteralType::Float => try!(symbol_table.find_type_or_err("f32")),
        LiteralType::Int => try!(symbol_table.find_type_or_err("i32")),
    };
    literal_expression.literal_type = Some(type_ref.clone());
    Ok(type_ref.clone())
}

#[cfg(test)]
mod tests {
    use super::*;
    use ::api::parse_module;

    #[test]
    fn test_check_struct() {
        let code = r#"
            struct Test {
                x: f32,
                y: f32,
            }
        "#;

        assert!(parse_module(code).is_ok());
    }

    #[test]
    fn test_check_struct_with_unknown_type() {
        let code = r#"
            struct Test {
                x: ThisTypeShouldNotExist,
            }
        "#;

        assert!(parse_module(code).is_err());
    }

    #[test]
    fn test_forward_declarations() {
        let code = r#"
            struct Test {
                x: Test2,
            }

            struct Test2 {
                y: f32,
            }
        "#;

        parse_module(code).unwrap();

        assert!(parse_module(code).is_ok());
    }

    #[test]
    fn test_check_constant_unknown() {
        let code = r#"
            const test: ThisTypeShouldNotExist;
        "#;

        assert!(parse_module(code).is_err());
    }

    #[test]
    fn test_check_constant() {
        let code = r#"
            const test: f32;
        "#;

        assert!(parse_module(code).is_ok());
    }

    #[test]
    fn test_check_sampler_unknown() {
        let code = r#"
            sampler test: ThisTypeShouldNotExist;
        "#;

        assert!(parse_module(code).is_err());
    }

    #[test]
    fn test_check_sampler() {
        let code = r#"
            sampler test: f32;
        "#;

        assert!(parse_module(code).is_ok());
    }

    #[test]
    fn test_check_function() {
        let code = r#"
            fn add(x: f32, y: f32) -> f32 {
                return x + y;
            }
        "#;

        assert!(parse_module(code).is_ok());
    }

    #[test]
    fn test_create_struct() {
        let code = r#"
            struct Vec4 {
                x: f32,
                y: f32,
                z: f32,
                w: f32,
            }

            fn vec4(x: f32, y: f32, z: f32, w: f32) -> Vec4 {
                return Vec4 {
                    x: x,
                    y: y,
                    z: z,
                    w: w,
                };
            }
        "#;

        assert!(parse_module(code).is_ok());
    }
}
