//! Defines a Solidity code emitter from a HIR.

use std::result;

use bulloak_syntax::utils::sanitize;

use crate::{
    config::Config,
    constants::INTERNAL_DEFAULT_INDENTATION,
    hir::{self, visitor::Visitor, Hir},
};

/// Solidity code emitter.
///
/// This struct holds the state of the emitter. It is not
/// tied to a specific HIR.
pub struct Emitter {
    /// The indentation level of the emitted code.
    indent: usize,
    /// The Solidity version to be used in the pragma directive.
    solidity_version: String,
}

impl Emitter {
    /// Create a new emitter with the given configuration.
    #[must_use]
    pub fn new(cfg: &Config) -> Self {
        Self {
            indent: INTERNAL_DEFAULT_INDENTATION,
            solidity_version: cfg.solidity_version.clone(),
        }
    }

    /// Emit Solidity code from the given HIR.
    #[must_use]
    pub fn emit(self, hir: &hir::Hir) -> String {
        EmitterI::new(self).emit(hir)
    }

    /// Return the indentation string. i.e. the string that is used
    /// to indent the emitted code.
    fn indent(&self) -> String {
        " ".repeat(self.indent)
    }
}

/// The internal implementation of the Solidity code emitter.
///
/// This emitter generates skeleton contracts and tests functions
/// inside that contract described in the input .tree file.
struct EmitterI {
    /// The emitter state.
    emitter: Emitter,
}

impl EmitterI {
    /// Create a new emitter with the given emitter state and modifier map.
    fn new(emitter: Emitter) -> Self {
        Self { emitter }
    }

    /// Emit Solidity code from the given HIR.
    ///
    /// This function is the entry point of the emitter. It is fine to unwrap
    /// here since emitting can't fail, otherwise a previous phase of the
    /// compiler is buggy.
    fn emit(&mut self, hir: &hir::Hir) -> String {
        match hir {
            Hir::Root(ref inner) => self.visit_root(inner).unwrap(),
            Hir::Contract(ref inner) => self.visit_contract(inner).unwrap(),
            Hir::Function(ref inner) => self.visit_function(inner).unwrap(),
            Hir::Comment(ref inner) => self.visit_comment(inner).unwrap(),
            Hir::Statement(_) => {
                unreachable!(
                    "a statement can't be a top-level source unit in Solidity"
                )
            }
        }
    }

    /// Emit the contract's definition header.
    ///
    /// This includes:
    /// - The Solidity version pragma.
    /// - The contract's name.
    fn emit_contract_header(
        &self,
        contract: &hir::ContractDefinition,
    ) -> String {
        let mut emitted = String::new();

        // It's fine to unwrap here because we check that the filename always
        // has an extension.
        let contract_name = sanitize(&contract.identifier);
        emitted.push_str(format!("contract {contract_name} {{\n").as_str());

        emitted
    }

    /// Emit a modifier.
    ///
    /// A modifier follows the structure:
    /// ```solidity
    /// modifier [MODIFIER_NAME]() {
    ///    _;
    /// }
    /// ```
    fn emit_modifier(&self, modifier: &str) -> String {
        let mut emitted = String::new();
        let indentation = self.emitter.indent();
        emitted.push_str(&format!("{indentation}modifier {modifier}() {{\n"));
        emitted.push_str(&format!("{}_;\n", indentation.repeat(2)));
        emitted.push_str(&format!("{indentation}}}\n"));
        emitted.push('\n');

        emitted
    }

    /// Emit a function's definition header.
    ///
    /// This includes:
    /// - The function's name.
    /// - The function's visibility.
    /// - Any modifiers that should be applied to the function.
    fn emit_fn_header(&self, function: &hir::FunctionDefinition) -> String {
        let mut emitted = String::new();

        let fn_indentation = self.emitter.indent();
        let fn_body_indentation = fn_indentation.repeat(2);

        let has_modifiers = function.modifiers.is_some();
        if has_modifiers {
            emitted.push_str(
                format!(
                    "{}function {}()\n",
                    fn_indentation, function.identifier
                )
                .as_str(),
            );
            emitted
                .push_str(format!("{fn_body_indentation}external\n").as_str());
        } else {
            emitted.push_str(
                format!("{}function {}()", fn_indentation, function.identifier)
                    .as_str(),
            );
            emitted.push_str(" external");
        }

        // Emit the modifiers that should be applied to this function.
        if let Some(ref modifiers) = function.modifiers {
            for modifier in modifiers {
                emitted.push_str(
                    format!("{fn_body_indentation}{modifier}\n").as_str(),
                );
            }
        }

        if has_modifiers {
            emitted.push_str(format!("{fn_indentation}{{\n").as_str());
        } else {
            emitted.push_str(" {\n");
        }

        emitted
    }
}

/// The visitor implementation for the emitter.
///
/// Note that the visitor is infallible because previous
/// passes ensure that the HIR is valid. In case an error
/// is found, it should be added to a previous pass.
impl Visitor for EmitterI {
    type CommentOutput = String;
    type ContractDefinitionOutput = String;
    type Error = ();
    type FunctionDefinitionOutput = String;
    type RootOutput = String;
    type StatementOutput = String;

    fn visit_root(
        &mut self,
        root: &hir::Root,
    ) -> result::Result<Self::RootOutput, Self::Error> {
        let mut emitted = String::new();
        emitted.push_str("// SPDX-License-Identifier: UNLICENSED\n");
        emitted.push_str(&format!(
            "pragma solidity {};\n\n",
            self.emitter.solidity_version
        ));

        for hir in &root.children {
            let result = match hir {
                Hir::Contract(contract) => self.visit_contract(contract)?,
                _ => unreachable!(),
            };

            emitted.push_str(&result);
        }

        Ok(emitted)
    }

    fn visit_contract(
        &mut self,
        contract: &hir::ContractDefinition,
    ) -> result::Result<Self::ContractDefinitionOutput, Self::Error> {
        let mut emitted = String::new();

        let contract_header = self.emit_contract_header(contract);
        emitted.push_str(&contract_header);

        for hir in &contract.children {
            if let Hir::Function(function) = hir {
                emitted.push_str(&self.visit_function(function)?);
            }
        }

        // Remove the last char, which is the extra '\n' from
        // emitting functions.
        emitted.pop();
        emitted.push('}');

        Ok(emitted)
    }

    fn visit_function(
        &mut self,
        function: &hir::FunctionDefinition,
    ) -> result::Result<Self::FunctionDefinitionOutput, Self::Error> {
        let mut emitted = String::new();

        if matches!(function.ty, hir::FunctionTy::Modifier) {
            emitted.push_str(&self.emit_modifier(&function.identifier));
        } else {
            let fn_header = self.emit_fn_header(function);
            emitted.push_str(&fn_header);

            if let Some(ref children) = function.children {
                for child in children {
                    if let Hir::Comment(comment) = child {
                        emitted.push_str(&self.visit_comment(comment)?);
                    } else if let Hir::Statement(statement) = child {
                        emitted.push_str(&self.visit_statement(statement)?);
                    }
                }
            }

            let indentation = self.emitter.indent();
            emitted.push_str(format!("{indentation}}}\n\n").as_str());
        }

        Ok(emitted)
    }

    fn visit_comment(
        &mut self,
        comment: &hir::Comment,
    ) -> result::Result<Self::CommentOutput, Self::Error> {
        let mut emitted = String::new();
        let indentation = self.emitter.indent().repeat(2);
        emitted
            .push_str(format!("{indentation}// {}\n", comment.lexeme).as_str());

        Ok(emitted)
    }

    fn visit_statement(
        &mut self,
        statement: &hir::Statement,
    ) -> result::Result<Self::StatementOutput, Self::Error> {
        let mut emitted = String::new();
        let indentation = self.emitter.indent().repeat(2);

        // Match any supported statement to its string representation
        match statement.ty {
            hir::StatementType::VmSkip => {
                emitted.push_str(
                    format!("{indentation}vm.skip(true);\n").as_str(),
                );
            }
        }

        Ok(emitted)
    }
}

#[cfg(test)]
mod tests {
    use pretty_assertions::assert_eq;

    use crate::{
        config::Config,
        hir::{translate, Hir, Statement, StatementType},
        scaffold::emitter,
    };

    fn scaffold(text: &str) -> anyhow::Result<String> {
        let cfg = Config::default();
        let hir = translate(text, &cfg)?;
        Ok(emitter::Emitter::new(&cfg).emit(&hir))
    }

    #[test]
    fn one_child() -> anyhow::Result<()> {
        let file_contents =
            String::from("FileTest\n└── when something bad happens\n   └── it should not revert");

        assert_eq!(
            &scaffold(&file_contents)?,
            r"// SPDX-License-Identifier: UNLICENSED
pragma solidity 0.8.0;

contract FileTest {
  function test_WhenSomethingBadHappens() external {
    // it should not revert
  }
}"
        );

        // Test that "it should revert" actions change the test name.
        let file_contents = String::from(
            "FileTest\n└── when something bad happens\n   └── it should revert",
        );

        assert_eq!(
            &scaffold(&file_contents)?,
            r"// SPDX-License-Identifier: UNLICENSED
pragma solidity 0.8.0;

contract FileTest {
  function test_RevertWhen_SomethingBadHappens() external {
    // it should revert
  }
}"
        );

        Ok(())
    }

    #[test]
    fn actions_without_conditions() -> anyhow::Result<()> {
        let file_contents = String::from(
            "FileTest\n├── it should do st-ff\n└── It never reverts.",
        );

        assert_eq!(
            &scaffold(&file_contents)?,
            r"// SPDX-License-Identifier: UNLICENSED
pragma solidity 0.8.0;

contract FileTest {
  function test_ShouldDoSt_ff() external {
    // it should do st-ff
  }

  function test_NeverReverts() external {
    // It never reverts.
  }
}"
        );

        let file_contents = String::from(
            "FileTest
├── it should do stuff
└── when something happens
    └── it should revert",
        );

        assert_eq!(
            &scaffold(&file_contents)?,
            r"// SPDX-License-Identifier: UNLICENSED
pragma solidity 0.8.0;

contract FileTest {
  function test_ShouldDoStuff() external {
    // it should do stuff
  }

  function test_RevertWhen_SomethingHappens() external {
    // it should revert
  }
}"
        );

        let file_contents = String::from(
            "FileTest
├── it should do stuff
├── when something happens
│   └── it should revert
└── it does everything",
        );

        assert_eq!(
            &scaffold(&file_contents)?,
            r"// SPDX-License-Identifier: UNLICENSED
pragma solidity 0.8.0;

contract FileTest {
  function test_ShouldDoStuff() external {
    // it should do stuff
  }

  function test_RevertWhen_SomethingHappens() external {
    // it should revert
  }

  function test_DoesEverything() external {
    // it does everything
  }
}"
        );

        Ok(())
    }

    #[test]
    fn unsanitized_input() -> anyhow::Result<()> {
        let file_contents =
            String::from("Fi-eTest\n└── when something bad happens\n   └── it should not revert");

        assert_eq!(
            &scaffold(&file_contents)?,
            r"// SPDX-License-Identifier: UNLICENSED
pragma solidity 0.8.0;

contract Fi_eTest {
  function test_WhenSomethingBadHappens() external {
    // it should not revert
  }
}"
        );

        Ok(())
    }

    #[test]
    fn indentation() -> anyhow::Result<()> {
        let file_contents =
            String::from("FileTest\n└── when something bad happens\n   └── it should not revert");

        assert_eq!(
            &scaffold(&file_contents)?,
            r"// SPDX-License-Identifier: UNLICENSED
pragma solidity 0.8.0;

contract FileTest {
  function test_WhenSomethingBadHappens() external {
    // it should not revert
  }
}"
        );

        Ok(())
    }

    #[test]
    fn with_vm_skip() -> anyhow::Result<()> {
        let file_contents = "FileTest\n└── when something bad happens\n   └── it should not revert";
        let mut cfg: Config = Config::default();
        cfg.emit_vm_skip = true;
        let hir = translate(file_contents, &cfg)?;
        let emitted = emitter::Emitter::new(&cfg).emit(&hir);

        assert_eq!(
            emitted,
            r"// SPDX-License-Identifier: UNLICENSED
pragma solidity 0.8.0;

contract FileTest {
  function test_WhenSomethingBadHappens() external {
    // it should not revert
    vm.skip(true);
  }
}"
        );

        Ok(())
    }

    #[test]
    #[should_panic]
    fn with_vm_skip_top_level_statement() {
        let hir = Hir::Statement(Statement { ty: StatementType::VmSkip });

        let _ = emitter::Emitter::new(&Config::default()).emit(&hir);
    }

    #[test]
    fn two_children() -> anyhow::Result<()> {
        let file_contents = String::from(
            r"TwoChildren_Test
├── when stuff called
│  └── it should revert
└── when not stuff called
   └── it should revert",
        );

        assert_eq!(
            &scaffold(&file_contents)?,
            r"// SPDX-License-Identifier: UNLICENSED
pragma solidity 0.8.0;

contract TwoChildren_Test {
  function test_RevertWhen_StuffCalled() external {
    // it should revert
  }

  function test_RevertWhen_NotStuffCalled() external {
    // it should revert
  }
}"
        );

        Ok(())
    }

    #[test]
    fn action_with_sibling_condition() -> anyhow::Result<()> {
        let file_contents = String::from(
            r"
Foo_Test
└── when stuff called
    ├── It should do stuff.
    ├── when a called
    │   └── it should revert
    ├── It should do more.
    └── when b called
        └── it should not revert",
        );

        assert_eq!(
            &scaffold(&file_contents)?,
            r"// SPDX-License-Identifier: UNLICENSED
pragma solidity 0.8.0;

contract Foo_Test {
  modifier whenStuffCalled() {
    _;
  }

  function test_WhenStuffCalled()
    external
    whenStuffCalled
  {
    // It should do stuff.
    // It should do more.
  }

  function test_RevertWhen_ACalled()
    external
    whenStuffCalled
  {
    // it should revert
  }

  function test_WhenBCalled()
    external
    whenStuffCalled
  {
    // it should not revert
  }
}"
        );

        Ok(())
    }

    #[test]
    fn action_recollection() -> anyhow::Result<()> {
        let file_contents = String::from(
            r"ActionsTest
└── when stuff called
   ├── it should do stuff
   ├── it should be cool
   └── it might break
",
        );

        assert_eq!(
            &scaffold(&file_contents)?,
            r"// SPDX-License-Identifier: UNLICENSED
pragma solidity 0.8.0;

contract ActionsTest {
  function test_WhenStuffCalled() external {
    // it should do stuff
    // it should be cool
    // it might break
  }
}"
        );

        Ok(())
    }

    #[test]
    fn first_action_revert_emits_revert_when() -> anyhow::Result<()> {
        let file_contents = String::from(
            r"ActionsTest
└── when stuff called
   ├── it should revert
   ├── it should be cool
   └── it might break
",
        );

        assert_eq!(
            &scaffold(&file_contents)?,
            r"// SPDX-License-Identifier: UNLICENSED
pragma solidity 0.8.0;

contract ActionsTest {
  function test_RevertWhen_StuffCalled() external {
    // it should revert
    // it should be cool
    // it might break
  }
}"
        );

        Ok(())
    }

    #[test]
    fn weird_it_should_revert_emits_revert_when() -> anyhow::Result<()> {
        let file_contents = String::from(
            r"ActionsTest
└── when stuff called
   └── IT sHould RevERT.
",
        );

        assert_eq!(
            &scaffold(&file_contents)?,
            r"// SPDX-License-Identifier: UNLICENSED
pragma solidity 0.8.0;

contract ActionsTest {
  function test_RevertWhen_StuffCalled() external {
    // IT sHould RevERT.
  }
}"
        );

        Ok(())
    }

    #[test]
    fn non_first_child_it_should_revert_doesnt_emit_revert_when(
    ) -> anyhow::Result<()> {
        let file_contents = String::from(
            r"ActionsTest
└── when stuff called
   ├── it should be cool
   ├── it should revert
   └── it might break
",
        );

        assert_eq!(
            &scaffold(&file_contents)?,
            r"// SPDX-License-Identifier: UNLICENSED
pragma solidity 0.8.0;

contract ActionsTest {
  function test_WhenStuffCalled() external {
    // it should be cool
    // it should revert
    // it might break
  }
}"
        );

        Ok(())
    }

    #[test]
    fn action_descriptions() -> anyhow::Result<()> {
        let file_contents = String::from(
            r"DescriptionsTest
└── when something bad happens
   └── it should try to revert
      ├── some stuff happened
      │  └── and that stuff
      └── was very _bad_",
        );

        assert_eq!(
            &scaffold(&file_contents)?,
            r"// SPDX-License-Identifier: UNLICENSED
pragma solidity 0.8.0;

contract DescriptionsTest {
  function test_WhenSomethingBadHappens() external {
    // it should try to revert
    //    some stuff happened
    //       and that stuff
    //    was very _bad_
  }
}"
        );

        Ok(())
    }

    #[test]
    fn deep_tree() -> anyhow::Result<()> {
        let file_contents = String::from(
            r#"DeepTest
├── when stuff called
│  └── it should revert
└── when not stuff called
   ├── when the deposit amount is zero
   │  └── it should revert
   └── when the deposit amount is not zero
      ├── when the number count is zero
      │  └── it should revert
      ├── when the asset is not a contract
      │  └── it should revert
      └── given the asset is a contract
          ├── when the asset misses the ERC-20 return value
          │  ├── it should create the child
          │  ├── it should perform the ERC-20 transfers
          │  └── it should emit a {MultipleChildren} event
          └── when the asset does not miss the ERC-20 return value
              ├── it should create the child
              └── it should emit a {MultipleChildren} event
                 ├── - Because the deposit should not be 0.
                 ├── - The number count is > 0.
                 └── - Events should be emitted."#,
        );

        assert_eq!(
            &scaffold(&file_contents)?,
            r"// SPDX-License-Identifier: UNLICENSED
pragma solidity 0.8.0;

contract DeepTest {
  function test_RevertWhen_StuffCalled() external {
    // it should revert
  }

  modifier whenNotStuffCalled() {
    _;
  }

  function test_RevertWhen_TheDepositAmountIsZero()
    external
    whenNotStuffCalled
  {
    // it should revert
  }

  modifier whenTheDepositAmountIsNotZero() {
    _;
  }

  function test_RevertWhen_TheNumberCountIsZero()
    external
    whenNotStuffCalled
    whenTheDepositAmountIsNotZero
  {
    // it should revert
  }

  function test_RevertWhen_TheAssetIsNotAContract()
    external
    whenNotStuffCalled
    whenTheDepositAmountIsNotZero
  {
    // it should revert
  }

  modifier givenTheAssetIsAContract() {
    _;
  }

  function test_WhenTheAssetMissesTheERC_20ReturnValue()
    external
    whenNotStuffCalled
    whenTheDepositAmountIsNotZero
    givenTheAssetIsAContract
  {
    // it should create the child
    // it should perform the ERC-20 transfers
    // it should emit a {MultipleChildren} event
  }

  function test_WhenTheAssetDoesNotMissTheERC_20ReturnValue()
    external
    whenNotStuffCalled
    whenTheDepositAmountIsNotZero
    givenTheAssetIsAContract
  {
    // it should create the child
    // it should emit a {MultipleChildren} event
    //    - Because the deposit should not be 0.
    //    - The number count is > 0.
    //    - Events should be emitted.
  }
}"
        );

        Ok(())
    }
}
