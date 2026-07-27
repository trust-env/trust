TODO

I want you to create a doc folder that'll be filled with all documentation of the project. This will not include the README.md file or CONTRIBUTING.md file. the doc folder will store all the how to use documentation that explains the rules of the Trust language, how to write it and all the features

TOCHECK

I want you to plan all the test implementation to avoid regression, create a plan that will cover all the features of the Trust language. The plan should include unit tests for individual components, integration tests for the overall system, and end-to-end tests that simulate real-world usage scenarios. The tests should be automated and run as part of the continuous integration process to ensure that any changes to the codebase do not introduce regressions or break existing functionality. I want also to have a coverage report that will show the percentage of code that is covered by tests, and highlight any areas that are not covered. The plan should also include a strategy for handling test data, including how to generate test data, how to store it, and how to clean it up after tests are run. Finally, the plan should include a process for reviewing and updating the tests as the Trust language evolves and new features are added.

Can you change the code generation so you are not using format!("...", var1, var2, etc..) and replace it by template files that will be read and rendered with the variables. This will make it easier to maintain and change the generated code without having to modify the Rust code itself.

I want you to imagine a concept of plugins for the Trust language. These plugins will be able to extend the language with new keywords, new types, and new features. The plugins will be written in any language and will be able to be loaded dynamically by the Trust compiler. The plugins will have to implement a specific interface that will allow them to register new keywords, types, and features with the compiler. The plugins will also be able to provide their own code generation templates that will be used by the compiler when generating code for the new features. Don't dev for now but imagine and explain to me 
