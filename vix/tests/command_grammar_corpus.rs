//! Parser stress corpus for command schemas. These deliberately model the
//! strange argv surfaces build derivations encounter; they are not claims that
//! Vix executes these tools yet.

use vix::VixParser;
use vix::ast::{CommandAtom, CommandPart, CommandPattern, Expr, Item};

struct Case {
    name: &'static str,
    program: &'static str,
    grammar: &'static str,
    usage: &'static str,
}

const CASES: &[Case] = &[
    Case {
        name: "Git",
        program: "git",
        grammar: "{global: String}* (clone [--depth {depth: Int}] [--filter={filter: String}] {url: String} {path: Path} | commit [--amend] [-m {message: String}] [--] {paths: Path}* | push [--force-with-lease[={lease: String}]] {remote: String} {refspec: String})",
        usage: "--no-pager clone --depth 1 --filter=blob:none https://example.invalid/repo.git {path}",
    },
    Case {
        name: "CMake",
        program: "cmake",
        grammar: "(-S {source: Path} -B {build: Path} [-G {generator: String}] [-D{define: String}]* [-U{unset: String}]* [--toolchain {toolchain: Path}] | --build {build: Path} [--target {target: String}]* [--config {config: String}] [--parallel {jobs: Int}] [-- {native: String}]*)",
        usage: "-S {path} -B build -G Ninja -DCMAKE_BUILD_TYPE=Release -DENABLE_LTO=ON --toolchain {path}",
    },
    Case {
        name: "Ninja",
        program: "ninja",
        grammar: "[-C {directory: Path}] [-f {file: Path}] [-j {jobs: Int}] [-k {failures: Int}] [-l {load: String}] [-d {debug: String}]* [-t {tool: String} [{tool_args: String}]*] [--] {targets: String}*",
        usage: "-C build -f build.ninja -j 16 -k 0 -d explain -d stats -- app tests",
    },
    Case {
        name: "Meson",
        program: "meson",
        grammar: "(setup {build: Path} [{source: Path}] [--backend {backend: String}] [--buildtype={kind: String}] [-D{option: String}]* [--cross-file {cross: Path}]* [--native-file {native: Path}]* | compile [-C {build: Path}] [-j {jobs: Int}] [--clean] {targets: String}* | install [-C {build: Path}] [--destdir {dest: Path}] [--tags {tags: String}])",
        usage: "setup build {path} --backend ninja --buildtype=release -Db_lto=true --cross-file {path} --native-file {path}",
    },
    Case {
        name: "Make",
        program: "make",
        grammar: "[-f {makefile: Path}]* [-C {directory: Path}]* [-j[{jobs: Int}]] [-l[{load: String}]] [-k] [-n] [-O{sync: String}] [{assignment: String}]* [--] {targets: String}*",
        usage: "-f {path} -C build -j16 -l2.5 -k -Otarget CC=clang V=1 -- all install",
    },
    Case {
        name: "Bazel",
        program: "bazel",
        grammar: "{startup: String}* (build | test | run | query | cquery) {targets: String}+ [--config={config: String}]* [--define={define: String}]* [--platforms={platform: String}] [--remote_cache={cache: String}] [--jobs={jobs: Int}] [--] {program_args: String}*",
        usage: "--output_user_root=/tmp/bazel build //app:all //lib:tests --config=release --define=ssl=boring --platforms=//platform:linux --jobs=12",
    },
    Case {
        name: "Buck2",
        program: "buck2",
        grammar: "[--isolation-dir {isolation: Path}] [--config {config: String}]* (build | test | run | query | audit) {targets: String}+ [--target-platforms {platform: String}] [--materializations {mode: String}] [--keep-going] [--] {args: String}*",
        usage: "--isolation-dir {path} --config build.mode=opt build //app:bin //lib/... --target-platforms root//platform:linux --materializations deferred --keep-going",
    },
    Case {
        name: "Gradle",
        program: "gradle",
        grammar: "{global: String}* [--] {tasks: String}+ [--rerun-tasks] [--continue] [--console={console: String}] [--configuration-cache] [--max-workers={workers: Int}] [-D{system: String}]* [-P{project: String}]* {task_options: String}*",
        usage: "--no-daemon --configuration-cache -Dorg.gradle.jvmargs=-Xmx2g -Pversion=1.0 -- :app:assemble :lib:test --tests=com.example.*",
    },
    Case {
        name: "Maven",
        program: "mvn",
        grammar: "[-f {pom: Path}] [-pl {projects: String}] [-am] [-amd] [-T {threads: String}] [-D{property: String}]* [-P{profiles: String}]* [--also-make] [--fail-at-end] {goals: String}+",
        usage: "-f {path} -pl :app,:core -am -T 2C -DskipTests=false -Drevision=1.2.3 -Prelease,sign --fail-at-end clean verify",
    },
    Case {
        name: "Ant",
        program: "ant",
        grammar: "[-buildfile {file: Path}] [-D{property: String}]* [-propertyfile {properties: Path}]* [-lib {classpath: String}]* [-logger {logger: String}] [-listener {listener: String}]* [-keep-going] [-noinput] {targets: String}*",
        usage: "-buildfile {path} -Dbuild.mode=release -Dtests=true -propertyfile {path} -lib libs/a.jar:libs/b.jar -keep-going -noinput clean dist",
    },
    Case {
        name: "Sbt",
        program: "sbt",
        grammar: "[-batch] [-no-colors] [-offline] [-sbt-dir {dir: Path}] [-ivy {ivy: Path}] [-D{property: String}]* [-J{jvm: String}]* [--client] {commands: String}+",
        usage: "-batch -no-colors -offline -sbt-dir {path} -ivy {path} -Dsbt.log.noformat=true -J-Xmx2g clean +test assembly",
    },
    Case {
        name: "Mill",
        program: "mill",
        grammar: "[-i] [-j {jobs: Int}] [--no-server] [--home {home: Path}] [-D{property: String}]* [--import {module: String}]* [--meta-level {level: Int}] {tasks: String}+ [--] {task_args: String}*",
        usage: "--no-server -j 8 --home {path} -Dmill.color=false --meta-level 1 __.compile app.test -- --test-only com.example.*",
    },
    Case {
        name: "Javac",
        program: "javac",
        grammar: "[-d {classes: Output<Path>}] [--class-path {classpath: String}] [--module-path {modulepath: String}] [--module-source-path {sources: String}] [--release {release: Int}] [-source {source: Int}] [-target {target: Int}] [-A{processor_option: String}]* [-J{jvm_option: String}]* [@{argfile: Input<Path>}]* {sources: Input<Path>}+",
        usage: "-d {path} --class-path libs/a.jar:libs/b.jar --module-path mods --release 25 -Amapstruct.verbose=true -J-Xmx1g @{path} {input}",
    },
    Case {
        name: "Jar",
        program: "jar",
        grammar: "(--create | --update | --extract | --list | --describe-module) [--file {archive: Output<Path>}] [--manifest {manifest: Input<Path>}] [--main-class {main: String}] [--module-version {version: String}] [--release {release: Int} {versioned: Input<Path>}*]* [-C {directory: Input<Path>} {files: Path}*]* {inputs: Input<Path>}*",
        usage: "--create --file {path} --manifest {input} --main-class com.example.Main --module-version 1.0 --release 21 -C {input} . {input}",
    },
    Case {
        name: "Jlink",
        program: "jlink",
        grammar: "--module-path {modules: String} --add-modules {roots: String} [--bind-services] [--compress={compression: String}] [--launcher {launcher: String}]* [--limit-modules {limited: String}] [--strip-debug] [--no-header-files] [--no-man-pages] --output {image: Output<Path>}",
        usage: "--module-path mods:jmods --add-modules app,java.logging --bind-services --compress=zip-6 --launcher app=app/com.example.Main --strip-debug --no-header-files --no-man-pages --output {path}",
    },
    Case {
        name: "Jpackage",
        program: "jpackage",
        grammar: "--name {name: String} [--type {kind: String}] [--input {input_dir: Input<Path>}] [--main-jar {jar: Path}] [--module-path {module_path: String}] [--module {module: String}] [--add-modules {modules: String}] [--java-options {java_option: String}]* [--arguments {argument: String}]* [--jlink-options {jlink_option: String}]* [--dest {dest: Output<Path>}]",
        usage: "--name Demo --type app-image --input {input} --main-jar app.jar --module-path mods --add-modules java.logging --java-options -Xmx1g --arguments --safe-mode --jlink-options --bind-services --dest {path}",
    },
    Case {
        name: "NativeImage",
        program: "native-image",
        grammar: "[-cp {classpath: String}] [-jar {jar: Input<Path>}] [--module-path {module_path: String}] [-H:Name={name: String}] [-H:Path={path: Output<Path>}] [-H:ConfigurationFileDirectories={configs: Input<Path>}]* [--initialize-at-build-time[={classes: String}]] [--initialize-at-run-time={runtime: String}] [--enable-url-protocols={protocols: String}] [-J{jvm: String}]* {main: String}",
        usage: "-cp app.jar:deps.jar -H:Name=demo -H:Path={path} -H:ConfigurationFileDirectories={input} --initialize-at-build-time=com.example --initialize-at-run-time=com.db --enable-url-protocols=http,https -J-Xmx4g com.example.Main",
    },
    Case {
        name: "Kotlinc",
        program: "kotlinc",
        grammar: "[-classpath {classpath: String}] [-d {output: Output<Path>}] [-include-runtime] [-jdk-home {jdk: Input<Path>}] [-jvm-target {target: String}] [-language-version {language: String}] [-api-version {api: String}] [-P {plugin: String}]* [-Xplugin={plugin_jar: Input<Path>}]* {sources: Input<Path>}+",
        usage: "-classpath libs.jar -d {path} -include-runtime -jdk-home {input} -jvm-target 21 -language-version 2.2 -P plugin:org.example:key=value -Xplugin={input} {input}",
    },
    Case {
        name: "ScalaCli",
        program: "scala-cli",
        grammar: "(compile | test | package | run) {inputs: Input<Path>}+ [--scala {version: String}] [--dependency {dependency: String}]* [--repository {repository: String}]* [--java-opt {java_option: String}]* [--scalac-option {scalac_option: String}]* [--power] [--] {program_args: String}*",
        usage: "package {input} --scala 3.7.1 --dependency org.typelevel::cats-core:2.13.0 --repository central --java-opt -Xmx2g --scalac-option -deprecation --power -- --serve",
    },
    Case {
        name: "Cargo",
        program: "cargo",
        grammar: "{global: String}* (build | test | rustc | install) [--package {package: String}]* [--target {target: String}] [--target-dir {target_dir: Output<Path>}] [--profile {profile: String}] [--features {features: String}]* [--no-default-features] [--message-format={format: String}] [--] {rustc_args: String}*",
        usage: "--locked build --package app --target aarch64-unknown-linux-gnu --target-dir {path} --profile release --features serde,simd --no-default-features --message-format=json-render-diagnostics -- -Ctarget-cpu=native",
    },
    Case {
        name: "Rustc",
        program: "rustc",
        grammar: "[--crate-name {name: String}] [--crate-type {kind: String}]* [--edition {edition: Int}] [--emit={emit: String}] [-C{codegen: String}]* [-Z{unstable: String}]* [--extern {external: String}]* [-L{search: String}]* [--cfg {cfg: String}]* [-o {output: Output<Path>}] {input: Input<Path>}",
        usage: "--crate-name demo --crate-type bin --edition 2024 --emit=link,metadata -Copt-level=3 -Clto=fat --extern serde=libserde.rlib -Ldependency=deps --cfg feature=simd -o {path} {input}",
    },
    Case {
        name: "Clang",
        program: "clang",
        grammar: "[-target {target: String}] [--sysroot={sysroot: Input<Path>}] [-std={standard: String}] [-O{level: String}] [-D{define: String}]* [-I{include: Input<Path>}]* [-Xclang {clang_arg: String}]* [-Wl,{linker_arg: String}]* [-c] {inputs: Input<Path>}+ [-o {output: Output<Path>}]",
        usage: "-target x86_64-linux-gnu --sysroot={input} -std=c23 -O3 -DNDEBUG=1 -I{input} -Xclang -fexperimental-new-pass-manager -Wl,--gc-sections -c {input} -o {path}",
    },
    Case {
        name: "Clangxx",
        program: "clang++",
        grammar: "[-stdlib={stdlib: String}] [-std={standard: String}] [-fmodules] [-fmodule-map-file={module_map: Input<Path>}]* [-isystem {system_include: Input<Path>}]* [-Xpreprocessor {preprocessor: String}]* [-Xlinker {linker: String}]* {inputs: Input<Path>}+ [-o {output: Output<Path>}]",
        usage: "-stdlib=libc++ -std=c++23 -fmodules -fmodule-map-file={input} -isystem {input} -Xpreprocessor -DTRACE -Xlinker --as-needed {input} -o {path}",
    },
    Case {
        name: "Gcc",
        program: "gcc",
        grammar: "[-x {language: String}] [-std={standard: String}] [-O{level: String}] [-f{feature: String}]* [-m{machine: String}]* [-D{define: String}]* [-I{include: Input<Path>}]* [-Wa,{assembler: String}]* [-Wl,{linker: String}]* [-c] {inputs: Input<Path>}+ [-o {output: Output<Path>}]",
        usage: "-x c -std=gnu23 -O2 -fPIC -fno-plt -march=x86-64-v3 -DNDEBUG -I{input} -Wa,--noexecstack -Wl,-z,now -c {input} -o {path}",
    },
    Case {
        name: "Gxx",
        program: "g++",
        grammar: "[-std={standard: String}] [-fabi-version={abi: Int}] [-fvisibility={visibility: String}] [-D{define: String}]* [-I{include: Input<Path>}]* [-isystem {system: Input<Path>}]* [-Wl,{linker: String}]* {inputs: Input<Path>}+ [-o {output: Output<Path>}]",
        usage: "-std=c++23 -fabi-version=19 -fvisibility=hidden -D_GLIBCXX_ASSERTIONS -I{input} -isystem {input} -Wl,--as-needed {input} -o {path}",
    },
    Case {
        name: "MsvcCl",
        program: "cl.exe",
        grammar: "[/nologo] [/c] [/std:{standard: String}] [/O{optimization: String}] [/D{define: String}]* [/I{include: Input<Path>}]* [/external:I{external: Input<Path>}]* [/Fo{object: Output<Path>}] [/Fd{pdb: Output<Path>}] [/sourceDependencies {deps: Output<Path>}] {sources: Input<Path>}+ [/link {link_args: String}*]",
        usage: "/nologo /c /std:c++latest /O2 /DNDEBUG /I{input} /external:I{input} /Fo{path} /Fd{path} /sourceDependencies {path} {input} /link /DEBUG:FULL",
    },
    Case {
        name: "MsvcLink",
        program: "link.exe",
        grammar: "[/NOLOGO] [/DLL] [/OUT:{output: Output<Path>}] [/PDB:{pdb: Output<Path>}] [/IMPLIB:{implib: Output<Path>}] [/LIBPATH:{libpath: Input<Path>}]* [/DEFAULTLIB:{defaultlib: String}]* [/WHOLEARCHIVE:{archive: Input<Path>}]* [/SUBSYSTEM:{subsystem: String}] [/OPT:{optimization: String}]* [@{response: Input<Path>}]* {objects: Input<Path>}+",
        usage: "/NOLOGO /DLL /OUT:{path} /PDB:{path} /IMPLIB:{path} /LIBPATH:{input} /DEFAULTLIB:kernel32.lib /WHOLEARCHIVE:{input} /SUBSYSTEM:CONSOLE /OPT:REF /OPT:ICF @{input} {input}",
    },
    Case {
        name: "MsvcLib",
        program: "lib.exe",
        grammar: "[/NOLOGO] [/OUT:{output: Output<Path>}] [/MACHINE:{machine: String}] [/DEF:{definition: Input<Path>}] [/LIBPATH:{search: Input<Path>}]* [/REMOVE:{member: String}]* [/EXTRACT:{member: String}]* [@{response: Input<Path>}]* {objects: Input<Path>}*",
        usage: "/NOLOGO /OUT:{path} /MACHINE:X64 /DEF:{input} /LIBPATH:{input} /REMOVE:old.obj /EXTRACT:keep.obj @{input} {input}",
    },
    Case {
        name: "Emcc",
        program: "emcc",
        grammar: "[-O{level: String}] [-s{setting: String}]* [-D{define: String}]* [-I{include: Input<Path>}]* [--preload-file {preload: Input<Path>}]* [--embed-file {embed: Input<Path>}]* [--js-library {library: Input<Path>}]* [--shell-file {shell: Input<Path>}] {inputs: Input<Path>}+ [-o {output: Output<Path>}]",
        usage: "-O3 -sWASM=1 -sALLOW_MEMORY_GROWTH=1 -DNDEBUG -I{input} --preload-file {input} --embed-file {input} --js-library {input} --shell-file {input} {input} -o {path}",
    },
    Case {
        name: "Zig",
        program: "zig",
        grammar: "(build-exe | build-lib | build-obj | cc | c++) {inputs: Input<Path>}+ [-target {target: String}] [-mcpu={cpu: String}] [-O {optimization: String}] [-D{define: String}]* [-I {include: Input<Path>}]* [-L {library_path: Input<Path>}]* [-l{library: String}]* [-femit-bin={output: Output<Path>}]",
        usage: "build-exe {input} -target x86_64-linux-musl -mcpu=baseline -O ReleaseSafe -DTRACE=1 -I {input} -L {input} -lpthread -femit-bin={path}",
    },
    Case {
        name: "Go",
        program: "go",
        grammar: "{global: String}* (build | test | install | generate) [-tags {tags: String}] [-mod={module_mode: String}] [-modfile={modfile: Input<Path>}] [-overlay={overlay: Input<Path>}] [-p {parallel: Int}] [-gcflags={gcflags: String}] [-ldflags={ldflags: String}] [-o {output: Output<Path>}] {packages: String}* [--] {test_args: String}*",
        usage: "build -tags netgo,osusergo -mod=vendor -modfile={input} -overlay={input} -p 8 -gcflags=all=-N -ldflags=-s,-w -o {path} ./cmd/...",
    },
    Case {
        name: "Swiftc",
        program: "swiftc",
        grammar: "[-module-name {module: String}] [-emit-module] [-emit-library] [-parse-as-library] [-target {target: String}] [-sdk {sdk: Input<Path>}] [-I {include: Input<Path>}]* [-L {library_path: Input<Path>}]* [-Xcc {cc_arg: String}]* [-Xlinker {linker_arg: String}]* [-o {output: Output<Path>}] {sources: Input<Path>}+",
        usage: "-module-name Demo -emit-module -emit-library -parse-as-library -target arm64-apple-macosx15.0 -sdk {input} -I {input} -L {input} -Xcc -fmodule-map-file=module.modulemap -Xlinker -rpath -o {path} {input}",
    },
    Case {
        name: "Xcodebuild",
        program: "xcodebuild",
        grammar: "[-project {project: Input<Path>} | -workspace {workspace: Input<Path>}] [-scheme {scheme: String}] [-configuration {configuration: String}] [-sdk {sdk: String}] [-destination {destination: String}]* [-derivedDataPath {derived: Output<Path>}] [-resultBundlePath {result: Output<Path>}] [-parallelizeTargets] [-jobs {jobs: Int}] {actions: String}+ {settings: String}*",
        usage: "-workspace {input} -scheme App -configuration Release -sdk iphonesimulator -destination platform=iOS Simulator,name=iPhone 17 -derivedDataPath {path} -resultBundlePath {path} -parallelizeTargets -jobs 8 clean build CODE_SIGNING_ALLOWED=NO ARCHS=arm64",
    },
    Case {
        name: "Msbuild",
        program: "msbuild.exe",
        grammar: "{projects: Input<Path>}+ [-target:{targets: String}] [-property:{property: String}]* [-maxCpuCount[={count: Int}]] [-restore] [-graphBuild[:{graph: String}]] [-binaryLogger[:{binlog: Output<Path>}]] [-fileLoggerParameters:{logging: String}]* [-verbosity:{verbosity: String}] [-warnAsError[:{codes: String}]] [@{response: Input<Path>}]*",
        usage: "{input} -target:Rebuild -property:Configuration=Release -property:Platform=x64 -maxCpuCount=8 -restore -graphBuild:true -binaryLogger:{path} -fileLoggerParameters:LogFile=build.log -verbosity:minimal -warnAsError:CS0618 @{input}",
    },
    Case {
        name: "Dotnet",
        program: "dotnet",
        grammar: "{global: String}* (build | test | publish | pack) [{project: Input<Path>}] [-c {configuration: String}] [-f {framework: String}] [-r {runtime: String}] [--self-contained {self_contained: String}] [-p:{property: String}]* [--artifacts-path {artifacts: Output<Path>}] [--no-restore] [--] {forwarded: String}*",
        usage: "publish {input} -c Release -f net10.0 -r linux-x64 --self-contained true -p:PublishSingleFile=true -p:ContinuousIntegrationBuild=true --artifacts-path {path} --no-restore -- --blame",
    },
    Case {
        name: "Python",
        program: "python",
        grammar: "[-B] [-E] [-I] [-O]* [-X {implementation: String}]* [-W {warning: String}]* ([-m {module: String}] | [{script: Input<Path>}]) {args: String}*",
        usage: "-B -I -OO -X dev -X utf8 -W error -m build --wheel --outdir {path}",
    },
    Case {
        name: "Pip",
        program: "pip",
        grammar: "(wheel | install | download) {requirements: String}* [-r {requirements_file: Input<Path>}]* [-c {constraints: Input<Path>}]* [--no-deps] [--no-build-isolation] [--config-settings {setting: String}]* [--platform {platform: String}]* [--python-version {python: String}] [--only-binary {binary: String}] [-w {wheel_dir: Output<Path>}]",
        usage: "wheel . -r {input} -c {input} --no-deps --no-build-isolation --config-settings=builddir=tmp --platform manylinux_2_28_x86_64 --python-version 3.14 --only-binary :all: -w {path}",
    },
    Case {
        name: "Node",
        program: "node",
        grammar: "[--conditions={condition: String}]* [--experimental-strip-types] [--import={import: String}]* [--loader={loader: String}]* [--require={require: String}]* [--env-file={env: Input<Path>}]* [--max-old-space-size={memory: Int}] ([--eval {code: String}] | [{script: Input<Path>}]) [--] {args: String}*",
        usage: "--conditions=development --experimental-strip-types --import=tsx --loader=custom-loader --require=source-map-support --env-file={input} --max-old-space-size=4096 {input} -- build --watch",
    },
    Case {
        name: "Npm",
        program: "npm",
        grammar: "[--prefix {prefix: Path}] [--workspace {workspace: String}]* [--workspaces] [--include-workspace-root] [--ignore-scripts] [--foreground-scripts] (ci | install | run | exec | pack) [{target: String}] [--] {args: String}*",
        usage: "--prefix {path} --workspace app --workspace packages/core --workspaces --include-workspace-root --ignore-scripts ci -- --audit=false",
    },
    Case {
        name: "Pnpm",
        program: "pnpm",
        grammar: "[-C {directory: Path}] [--filter {filter: String}]* [-r] [--workspace-concurrency {concurrency: Int}] [--frozen-lockfile] [--offline] (install | build | deploy | exec) [{target: String}] [--] {args: String}*",
        usage: "-C {path} --filter ./packages/** --filter !./packages/legacy -r --workspace-concurrency 8 --frozen-lockfile --offline build -- --mode production",
    },
    Case {
        name: "Vite",
        program: "vite",
        grammar: "(build | optimize) [--config {config: Input<Path>}] [--base {base: String}] [--mode {mode: String}] [--logLevel {log_level: String}] [--clearScreen {clear: String}] [--configLoader {loader: String}] [--debug [{debug: String}]] [--filter {filter: String}] [--minify [{minify: String}]] [--sourcemap [{sourcemap: String}]] [--watch]",
        usage: "build --config {input} --base /assets/ --mode production --logLevel info --clearScreen false --configLoader runner --debug plugin-transform --filter src/** --minify esbuild --sourcemap hidden --watch",
    },
    Case {
        name: "Rolldown",
        program: "rolldown",
        grammar: "[-c {config: Input<Path>}] [--configPlugin {plugin: String}]* [-d {directory: Output<Path>} | -o {file: Output<Path>}] [-f {format: String}] [-e {external: String}]* [-g {globals: String}]* [--define {define: String}]* [--inject {inject: Input<Path>}]* [--minify] [--sourcemap [{sourcemap: String}]] [--watch] {inputs: Input<Path>}*",
        usage: "-c {input} --configPlugin tsx -d {path} -f esm -e react -e react-dom -g react=React --define process.env.NODE_ENV=production --inject {input} --minify --sourcemap hidden --watch {input}",
    },
    Case {
        name: "Esbuild",
        program: "esbuild",
        grammar: "{entries: Input<Path>}+ [--bundle] [--platform={platform: String}] [--format={format: String}] [--target={target: String}]* [--external:{external: String}]* [--loader:{extension: String}={loader: String}]* [--define:{key: String}={value: String}]* [--banner:{kind: String}={banner: String}]* [--outdir={outdir: Output<Path>} | --outfile={outfile: Output<Path>}] [--sourcemap[={sourcemap: String}]]",
        usage: "{input} --bundle --platform=node --format=esm --target=node24 --external:sharp --loader:.wasm=file --define:process.env.NODE_ENV=production --banner:js=/*built*/ --outdir={path} --sourcemap=external",
    },
    Case {
        name: "Webpack",
        program: "webpack",
        grammar: "[--config {config: Input<Path>}]* [--config-name {name: String}]* [--env {environment: String}]* [--mode {mode: String}] [--entry {entry: Input<Path>}]* [--output-path {output: Output<Path>}] [--output-filename {filename: String}] [--target {target: String}]* [--define-process-env-node-env {node_env: String}] [--json[={stats: Output<Path>}]] [--] {entries: Input<Path>}*",
        usage: "--config {input} --config-name client --env production=true --mode production --entry {input} --output-path {path} --output-filename app.[contenthash].js --target web --target es2022 --define-process-env-node-env production --json={path}",
    },
    Case {
        name: "Tsc",
        program: "tsc",
        grammar: "[-p {project: Input<Path>}] [--build {projects: Input<Path>}*] [--target {target: String}] [--module {module: String}] [--moduleResolution {resolution: String}] [--lib {libraries: String}] [--types {types: String}] [--outDir {out: Output<Path>}] [--declaration] [--declarationMap] [--incremental] [--tsBuildInfoFile {info: Output<Path>}] {inputs: Input<Path>}*",
        usage: "-p {input} --build {input} --target ES2024 --module NodeNext --moduleResolution NodeNext --lib ES2024,DOM --types node --outDir {path} --declaration --declarationMap --incremental --tsBuildInfoFile {path}",
    },
    Case {
        name: "Protoc",
        program: "protoc",
        grammar: "[-I{proto_path: Input<Path>}]* [--descriptor_set_out={descriptor: Output<Path>}] [--include_imports] [--include_source_info] [--dependency_out={deps: Output<Path>}] [--plugin=protoc-gen-{plugin_name: String}={plugin: Input<Path>}]* [--{language: String}_out={options: String}:{output: Output<Path>}]* [--{language_opt: String}_opt={plugin_option: String}]* {inputs: Input<Path>}+",
        usage: "-I{input} --descriptor_set_out={path} --include_imports --include_source_info --dependency_out={path} --plugin=protoc-gen-tonic={input} --rust_out=experimental-codegen=enabled:{path} --rust_opt=paths=source_relative {input}",
    },
    Case {
        name: "Autoconf",
        program: "autoconf",
        grammar: "[-I {include: Input<Path>}]* [-B {prepend: Input<Path>}]* [-W {warning: String}]* [-f] [-v] [-d] [-o {output: Output<Path>}] [{template: Input<Path>}]",
        usage: "-I {input} -B {input} -W all -W no-obsolete -f -v -d -o {path} {input}",
    },
    Case {
        name: "Automake",
        program: "automake",
        grammar: "[--add-missing] [--copy] [--force-missing] [--foreign | --gnu | --gnits] [--include-deps] [--no-force] [--warnings={warnings: String}]* [-W{warning: String}]* [-a] [-c] {makefiles: Input<Path>}*",
        usage: "--add-missing --copy --force-missing --foreign --include-deps --warnings=all --warnings=no-portability -Woverride -a -c {input}",
    },
    Case {
        name: "Libtool",
        program: "libtool",
        grammar: "(--mode={mode: String}) [--tag={tag: String}] [--silent] [--preserve-dup-deps] [--no-undefined] [-version-info {version: String}] [-rpath {rpath: Path}] [-module] [-avoid-version] [-o {output: Output<Path>}] {compiler: String} {args: String}*",
        usage: "--mode=link --tag=CC --silent --preserve-dup-deps --no-undefined -version-info 1:0:0 -rpath /lib -module -avoid-version -o {path} cc {input} -lm",
    },
    Case {
        name: "PkgConfig",
        program: "pkg-config",
        grammar: "[--static] [--cflags] [--libs] [--modversion] [--variable={variable: String}] [--define-variable={name: String}={value: String}]* [--with-path={path: Input<Path>}]* [--env-only] [--validate] [--print-requires-private] {packages: String}+",
        usage: "--static --cflags --libs --define-variable=prefix=/opt/sdk --with-path={input} --env-only --validate --print-requires-private openssl libcurl>=8.0",
    },
    Case {
        name: "Tar",
        program: "tar",
        grammar: "({old_style: String} | -c | -x | -t) [-f {archive: Path}] [-C {directory: Path}]* [--transform={transform: String}]* [--owner={owner: String}] [--group={group: String}] [--mtime={mtime: String}] [--sort={sort: String}] [--numeric-owner] [--strip-components={strip: Int}] [-I {compressor: String}] [--] {members: Path}*",
        usage: "cavf {path} -C {input} --transform=s,^./,, --owner=0 --group=0 --mtime=@0 --sort=name --numeric-owner --strip-components=1 -I zstd -- .",
    },
    Case {
        name: "Zip",
        program: "zip",
        grammar: "[-r] [-q] [-X] [-0 | -1 | -9] [-x {exclude: String}*] [-i {include: String}*] [-j] [-y] [--symlinks] [--names-stdin] {archive: Output<Path>} {inputs: Input<Path>}*",
        usage: "-r -q -X -9 -x *.tmp *.bak -i src/** assets/** -j -y --symlinks {path} {input}",
    },
    Case {
        name: "Patch",
        program: "patch",
        grammar: "[-p{strip: Int}] [-d {directory: Path}] [-i {patch: Input<Path>}] [-o {output: Output<Path>}] [-R] [-N] [-f | -t] [--merge[={merge: String}]] [--binary] [--fuzz={fuzz: Int}] [--set-time] [{original: Input<Path>}]",
        usage: "-p1 -d {path} -i {input} -o {path} -N -t --merge=diff3 --binary --fuzz=2 --set-time {input}",
    },
    Case {
        name: "Curl",
        program: "curl",
        grammar: "[-L] [--fail-with-body] [--retry {retries: Int}] [--retry-all-errors] [--connect-timeout {timeout: Int}] [-H {header: String}]* [-d {data: String}]* [--data-binary @{data_file: Input<Path>}]* [--cacert {ca: Input<Path>}] [--cert {cert: Input<Path>}] [-o {output: Output<Path>}] {urls: String}+",
        usage: "-L --fail-with-body --retry 5 --retry-all-errors --connect-timeout 30 -H Accept:application/json -H X-Build:1 -d mode=release --data-binary @{input} --cacert {input} --cert {input} -o {path} https://example.invalid/artifact",
    },
    Case {
        name: "Patchelf",
        program: "patchelf",
        grammar: "[--set-interpreter {interpreter: Path}] [--set-soname {soname: String}] [--set-rpath {rpath: String}] [--add-rpath {add_rpath: String}]* [--remove-needed {remove: String}]* [--add-needed {add: String}]* [--replace-needed {old: String} {new: String}]* [--shrink-rpath] [--allowed-rpath-prefixes {prefixes: String}] [--no-default-lib] {files: Input<Path>}+",
        usage: "--set-interpreter /lib/ld-linux.so --set-soname libdemo.so.1 --set-rpath $ORIGIN/../lib --add-rpath /opt/lib --remove-needed libold.so --add-needed libnew.so --replace-needed liba.so libb.so --shrink-rpath --allowed-rpath-prefixes /nix/store --no-default-lib {input}",
    },
    Case {
        name: "Ffmpeg",
        program: "ffmpeg",
        grammar: "[-hide_banner] [-loglevel {level: String}] [-y | -n] [-thread_queue_size {queue: Int}]* (-f {input_format: String})* (-ss {input_seek: String})* (-i {input: Input<Path>})+ [-filter_complex {filtergraph: String}] [-map {mapping: String}]* [-map_metadata[:{metadata_out: String}] {metadata_in: String}]* [-c:{stream: String} {codec: String}]* [-b:{bitstream: String} {bitrate: String}]* [-metadata[:{metadata_stream: String}] {metadata: String}]* [-movflags {movflags: String}] [-f {output_format: String}] {output: Output<Path>}+",
        usage: "-hide_banner -loglevel warning -y -thread_queue_size 1024 -f lavfi -i {input} -ss 00:00:03 -i {input} -filter_complex {text} -map 0:v:0 -map 1:a:0? -map_metadata:s:a 1:g -c:v libx265 -c:a copy -b:v 4M -metadata:s:a:0 language=eng -movflags +faststart -f matroska {path}",
    },
    Case {
        name: "WasmOpt",
        program: "wasm-opt",
        grammar: "[-O | -O{level: Int} | -O{s: String}] [--enable-{feature: String}]* [--disable-{disabled: String}]* [--pass-arg={pass_arg: String}]* [--closed-world] [--strip-debug] [--strip-dwarf] [--emit-target-features] [--output-source-map={source_map: Output<Path>}] [-o {output: Output<Path>}] {input: Input<Path>}",
        usage: "-O4 --enable-simd --enable-bulk-memory --disable-multivalue --pass-arg=asyncify-imports@env.foo --closed-world --strip-debug --strip-dwarf --emit-target-features --output-source-map={path} -o {path} {input}",
    },
    Case {
        name: "Cabal",
        program: "cabal",
        grammar: "{global: String}* (build | test | haddock | install) {targets: String}* [--project-file={project: Input<Path>}] [--builddir={build: Output<Path>}] [--enable-tests] [--enable-benchmarks] [--constraint={constraint: String}]* [--allow-newer={newer: String}]* [--ghc-option={ghc: String}]* [--offline] [--] {args: String}*",
        usage: "v2-build all --project-file={input} --builddir={path} --enable-tests --enable-benchmarks --constraint=bytestring>=0.12 --allow-newer=base --ghc-option=-O2 --offline -- -j8",
    },
    Case {
        name: "Stack",
        program: "stack",
        grammar: "[--stack-yaml {yaml: Input<Path>}] [--resolver {resolver: String}] [--compiler {compiler: String}] [--work-dir {work: Output<Path>}] [--system-ghc] [--no-install-ghc] (build | test | install) {targets: String}* [--flag {flag: String}]* [--ghc-options {ghc: String}]* [--] {args: String}*",
        usage: "--stack-yaml {input} --resolver lts-24.0 --compiler ghc-9.12.2 --work-dir {path} --system-ghc --no-install-ghc build app:test --flag app:simd --ghc-options -O2 -- -j8",
    },
    Case {
        name: "Dune",
        program: "dune",
        grammar: "[--root {root: Path}] [--build-dir {build: Output<Path>}] [--workspace {workspace: Input<Path>}] [--profile {profile: String}] [--display {display: String}] [--cache {cache: String}] (build | runtest | install | exec) {targets: String}* [--only-packages {packages: String}] [--ignore-promoted-rules] [--] {args: String}*",
        usage: "--root {path} --build-dir {path} --workspace {input} --profile release --display short --cache enabled build @install @runtest --only-packages app,lib --ignore-promoted-rules",
    },
    Case {
        name: "Opam",
        program: "opam",
        grammar: "[--root {root: Path}] [--switch {switch: String}] [--jobs {jobs: Int}] [--criteria={criteria: String}] [--solver={solver: String}] (install | pin | exec) {packages: String}* [--deps-only] [--locked] [--with-test] [--with-doc] [--assume-depexts] [--] {args: String}*",
        usage: "--root {path} --switch 5.3.0 --jobs 8 --criteria=-removed,-notuptodate --solver=0install install . --deps-only --locked --with-test --with-doc --assume-depexts",
    },
    Case {
        name: "Mix",
        program: "mix",
        grammar: "[--no-archives-check] [--no-deps-check] [--no-elixir-version-check] (compile | test | release | escript.build) [--force] [--warnings-as-errors] [--no-deps-loading] [--stale] [--cover] [--trace] [--only {tag: String}]* [--exclude {exclude: String}]* [--] {files: Input<Path>}*",
        usage: "--no-archives-check --no-deps-check compile --force --warnings-as-errors --no-deps-loading --stale --cover --trace --only integration --exclude flaky -- {input}",
    },
    Case {
        name: "Rebar3",
        program: "rebar3",
        grammar: "[--config {config: Input<Path>}] [--profile {profile: String}] [--offline] [--paths {paths: String}]* (compile | eunit | ct | release | tar) [--apps {apps: String}] [--suite {suite: String}]* [--case {case: String}]* [--dir {directory: Path}] [--] {args: String}*",
        usage: "--config {input} --profile prod --offline --paths _build/default/lib compile --apps app,core --suite integration_SUITE --case builds --dir {path}",
    },
    Case {
        name: "Bundle",
        program: "bundle",
        grammar: "[--gemfile={gemfile: Input<Path>}] [--path={path: Output<Path>}] [--without={without: String}] [--with={with: String}] [--jobs={jobs: Int}] [--retry={retry: Int}] [--local] [--deployment] (install | exec | package) [{command: String}] [--] {args: String}*",
        usage: "--gemfile={input} --path={path} --without=development:test --with=build --jobs=8 --retry=3 --local --deployment install -- --clean",
    },
    Case {
        name: "Rake",
        program: "rake",
        grammar: "[-f {rakefile: Input<Path>}] [-C {directory: Path}] [-I{libdir: Input<Path>}]* [-r{require: String}]* [-j {jobs: Int}] [-m] [-n] [-t] [{environment: String}]* {tasks: String}+",
        usage: "-f {input} -C {path} -I{input} -rcompiler -j 8 -m -n -t MODE=release CC=clang compile:all test:unit",
    },
    Case {
        name: "DockerBuildx",
        program: "docker",
        grammar: "buildx build [--builder {builder: String}] [--platform {platforms: String}] [--build-arg {build_arg: String}]* [--secret {secret: String}]* [--ssh {ssh: String}]* [--cache-from {cache_from: String}]* [--cache-to {cache_to: String}]* [--output {output: String}]* [-f {dockerfile: Input<Path>}] [--target {target: String}] {context: Input<Path>}",
        usage: "buildx build --builder nix --platform linux/amd64,linux/arm64 --build-arg VERSION=1 --secret id=npmrc,src=.npmrc --ssh default --cache-from type=local,src=cache --cache-to type=local,dest=cache-new --output type=oci,dest=image.tar -f {input} --target runtime {input}",
    },
    Case {
        name: "Qemu",
        program: "qemu-system-x86_64",
        grammar: "[-machine {machine: String}] [-cpu {cpu: String}] [-smp {smp: String}] [-m {memory: String}] [-kernel {kernel: Input<Path>}] [-initrd {initrd: Input<Path>}] [-append {append: String}] [-drive {drive: String}]* [-device {device: String}]* [-netdev {netdev: String}]* [-chardev {chardev: String}]* [-nographic]",
        usage: "-machine q35,accel=tcg -cpu max -smp 4,sockets=1 -m 2G -kernel {input} -initrd {input} -append console=ttyS0 -drive file=disk.img,if=virtio,format=raw -device virtio-net-pci,netdev=n0 -netdev user,id=n0 -chardev stdio,id=char0 -nographic",
    },
    Case {
        name: "Install",
        program: "install",
        grammar: "[-D] [-d] [-m {mode: String}] [-o {owner: String}] [-g {group: String}] [-s] [-p] [-t {target_directory: Output<Path>}] [--strip-program={strip: String}] [--compare] [--backup={backup: String}] {sources: Input<Path>}+ [{destination: Output<Path>}]",
        usage: "-D -m 0755 -o root -g root -s -p -t {path} --strip-program=llvm-strip --compare --backup=numbered {input}",
    },
    Case {
        name: "Ld",
        program: "ld.lld",
        grammar: "[-m {emulation: String}] [--sysroot={sysroot: Input<Path>}] [-L{search: Input<Path>}]* [-l{library: String}]* [--whole-archive {archives: Input<Path>}* --no-whole-archive] [--start-group {group: Input<Path>}* --end-group] [-T {script: Input<Path>}] [--gc-sections] [--build-id={build_id: String}] [--dynamic-linker {loader: Path}] [-o {output: Output<Path>}] {objects: Input<Path>}+",
        usage: "-m elf_x86_64 --sysroot={input} -L{input} -lc -lm --whole-archive {input} --no-whole-archive --start-group {input} --end-group -T {input} --gc-sections --build-id=sha1 --dynamic-linker /lib/ld.so -o {path} {input}",
    },
];

fn pattern_term_count(pattern: &CommandPattern) -> usize {
    pattern
        .alternatives
        .iter()
        .map(|alternative| {
            alternative
                .terms
                .iter()
                .map(|term| {
                    1 + match &term.atom {
                        CommandAtom::Optional(optional) => pattern_term_count(&optional.pattern),
                        CommandAtom::Group(group) => pattern_term_count(&group.pattern),
                        CommandAtom::Literal(_) | CommandAtom::Slot(_) => 0,
                    }
                })
                .sum::<usize>()
        })
        .sum()
}

#[test]
fn parses_at_least_fifty_chunky_real_world_command_schemas_and_usages() {
    assert!(
        CASES.len() >= 50,
        "corpus shrank to {} commands",
        CASES.len()
    );
    let parser = VixParser::new();

    for case in CASES {
        let variable = case.name.to_ascii_lowercase();
        let source = format!(
            "command {} -> Tree {{ program {:?} grammar {{ {} }} }}\nfn exercise({}: {}, input: Tree, path: Path, text: String) -> Tree {{ {}! {{ {} }} }}",
            case.name, case.program, case.grammar, variable, case.name, variable, case.usage,
        );
        let file = parser.parse(&source).unwrap_or_else(|error| {
            panic!("{} schema or usage failed:\n{error}\n{source}", case.name)
        });
        assert_eq!(file.items.len(), 2, "{} fixture item count", case.name);
        let Item::Command(command) = &file.items[0] else {
            panic!("{} schema", case.name);
        };
        assert!(
            pattern_term_count(&command.grammar.pattern) >= 8,
            "{} schema stopped being chunky",
            case.name,
        );
        let Item::Fn(function) = &file.items[1] else {
            panic!("{} usage function", case.name);
        };
        let Some(Expr::Command(usage)) = &function.body.tail else {
            panic!("{} usage", case.name);
        };
        assert!(
            usage.parts.len() >= 6,
            "{} usage stopped being chunky",
            case.name
        );
    }
}

#[test]
fn pathological_command_tokens_survive_as_argv_instead_of_language_syntax() {
    let source = r#"
fn edge_cases(tool: Tool, path: Path) -> Tree {
    let cpp = tool! { -std=c++23 -stdlib=libc++ };
    let labels = tool! { //app:all //lib/tests:unit };
    let streams = tool! { -map 0:a? -movflags +faststart };
    tool! { /OUT:{path} /DEBUG:FULL @response.rsp }
}
"#;
    let file = VixParser::new().parse(source).expect("edge tokens parse");
    let Item::Fn(function) = &file.items[0] else {
        panic!("fixture is a function");
    };
    let token_values = |expr: &Expr| {
        let Expr::Command(command) = expr else {
            panic!("fixture expression is a command");
        };
        command
            .parts
            .iter()
            .filter_map(|part| match part {
                CommandPart::Token(token) => Some(token.value.clone()),
                CommandPart::Splice(_) => None,
            })
            .collect::<Vec<_>>()
    };

    let vix::ast::Stmt::Let(cpp) = &function.body.stmts[0] else {
        panic!("cpp binding");
    };
    assert_eq!(token_values(&cpp.value), ["-std=c++23", "-stdlib=libc++"]);
    let vix::ast::Stmt::Let(labels) = &function.body.stmts[1] else {
        panic!("labels binding");
    };
    assert_eq!(
        token_values(&labels.value),
        ["//app:all", "//lib/tests:unit"]
    );
    let vix::ast::Stmt::Let(streams) = &function.body.stmts[2] else {
        panic!("streams binding");
    };
    assert_eq!(
        token_values(&streams.value),
        ["-map", "0:a?", "-movflags", "+faststart"]
    );
    assert_eq!(
        token_values(function.body.tail.as_ref().expect("tail command")),
        ["/OUT:", "/DEBUG:FULL", "@response.rsp"]
    );
}

#[test]
fn malformed_command_declarations_and_usages_are_rejected() {
    let parser = VixParser::new();
    let invalid = [
        "command Bad -> Tree { grammar { --flag } }",
        "command Bad -> Tree { program \"bad\" { --flag } }",
        "command Bad -> Tree { program \"bad\" grammar { {missing_type} } }",
        "command Bad -> Tree { program \"bad\" grammar { {name: } } }",
        "command Bad -> Tree { program \"bad\" grammar { [--flag } }",
        "command Bad -> Tree { program \"bad\" grammar { (--a | --b } }",
        "command Bad -> Tree { program \"bad\" grammar { --a | } }",
        "command Bad -> Tree { program \"bad\" grammar { [] } }",
        "fn bad(tool: Bad) -> Tree { tool! { --flag {value } }",
        "fn bad(tool: Bad) -> Tree { tool! { --flag {{value}} } }",
        "fn bad(tool: Bad) -> Tree { tool! { --flag } extra }",
    ];

    for source in invalid {
        assert!(
            parser.parse(source).is_err(),
            "unexpectedly accepted:\n{source}"
        );
    }
}
