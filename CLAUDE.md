目标：一套代码，同时支持VST3、AU、CLAP三种插件格式，且能在Windows、macOS、Linux三大平台上运行。

要求：
0. 及时将进展log到文件。
1. _sys是底层系统binding，_rs是rust安全包装，sunmao是融合抽象，检查问题应该从底层开始，逐层往上。
2. 仓库目前的代码不代表是成功的代码，可能是未完成或者有缺陷的代码。
3. VST3和CLAP支持全平台，AU只支持macOS。



额外目标：完成/tools/sunmao_unittest_runner，一个用来测试的最小化的宿主，能加载插件并运行单元测试和手动测试，能输出详细日志，并且能在三大平台上运行，支持GUI和命令行两种模式。