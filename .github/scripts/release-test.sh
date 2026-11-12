#!/usr/bin/env bash
# release.sh 的自测：在临时 git 仓库里构造各种提交组合，核对版本计算与文件改写。
# 用法：.github/scripts/release-test.sh（CI 的 scripts 任务也会执行）
set -euo pipefail

# 与运行环境隔离：CI 里的 GITHUB_* 会改变输出位置和链接格式，各用例按需显式设置。
unset GITHUB_OUTPUT GITHUB_REPOSITORY GITHUB_SERVER_URL NOTES_FILE

SRC="$(git rev-parse --show-toplevel)"
SCRIPT="${SRC}/.github/scripts/release.sh"
WORK="$(mktemp -d)"
trap 'rm -rf "$WORK"' EXIT
FAILS=0
CASES=0

# 用真实的版本文件搭一个最小仓库，起始版本 0.1.0。
new_repo() {
    local dir="${WORK}/$1" f
    mkdir -p "$dir"
    for f in Cargo.toml Cargo.lock x-loadgen-bin/x-loadgen/web/package.json \
        x-loadgen-bin/x-loadgen/openapi.json deploy/config/.env.example deploy/README.md; do
        mkdir -p "$(dirname "${dir}/${f}")"
        cp "${SRC}/${f}" "${dir}/${f}"
    done
    (
        cd "$dir"
        "$SCRIPT_APPLY_FIXTURE" 0.1.0
        git init -q -b main
        git config user.name test
        git config user.email test@example.invalid
        git add -A
        git commit -qm "第 01 步：基线"
    )
    printf '%s' "$dir"
}

commit() {
    local dir="$1" subject="$2" body="${3:-}"
    if [ -n "$body" ]; then
        git -C "$dir" commit -q --allow-empty -m "$subject" -m "$body"
    else
        git -C "$dir" commit -q --allow-empty -m "$subject"
    fi
}

plan() {
    local dir="$1"
    (cd "$dir" && "$SCRIPT" plan)
}

expect() {
    local name="$1" dir="$2" want="$3" got
    CASES=$((CASES + 1))
    got="$(plan "$dir" | grep -E '^(release|version|bump)=' | paste -sd' ')"
    if [ "$got" = "$want" ]; then
        echo "ok   ${name}: ${got}"
    else
        echo "FAIL ${name}: want '${want}' got '${got}'"
        FAILS=$((FAILS + 1))
    fi
}

check() {
    local name="$1"
    shift
    CASES=$((CASES + 1))
    if "$@"; then
        echo "ok   ${name}"
    else
        echo "FAIL ${name}"
        FAILS=$((FAILS + 1))
    fi
}

# 夹具：把复制来的版本文件统一改成 0.1.0，测试与仓库当前版本无关。
SCRIPT_APPLY_FIXTURE="${WORK}/fixture.sh"
cat >"$SCRIPT_APPLY_FIXTURE" <<'SH'
#!/usr/bin/env bash
set -euo pipefail
v="$1"
sed -E -i "/^\[workspace\.package\]/,/^\[/ s/^version = \"[^\"]*\"/version = \"${v}\"/" Cargo.toml
sed -E -i "s/^(x-loadgen-[a-z]+ = \{ version = \")[^\"]*(\")/\1${v}\2/" Cargo.toml
awk -v v="$v" '/^name = "x-loadgen(-[a-z]+)?"$/{h=1;print;next} h&&/^version = "/{print "version = \"" v "\"";h=0;next}{print}' Cargo.lock >Cargo.lock.new
mv Cargo.lock.new Cargo.lock
sed -E -i "0,/\"version\": \"[^\"]*\"/s//\"version\": \"${v}\"/" x-loadgen-bin/x-loadgen/web/package.json
sed -E -i "0,/\"version\": \"[^\"]*\"/s//\"version\": \"${v}\"/" x-loadgen-bin/x-loadgen/openapi.json
sed -E -i "s/^X_LOADGEN_VERSION=.*/X_LOADGEN_VERSION=${v}/" deploy/config/.env.example
sed -E -i "s/^(\| \`X_LOADGEN_VERSION\` \| \`)[^\`]*(\`)/\1${v}\2/" deploy/README.md
SH
chmod +x "$SCRIPT_APPLY_FIXTURE"

# 1. 没有任何 tag：按当前版本首次发版，与提交类型无关。
r="$(new_repo initial)"
commit "$r" "第 02 步：非约定式提交"
expect "无 tag 首次发版" "$r" "release=true version=0.1.0 bump=initial"
(cd "$r" && "$SCRIPT" apply 0.1.0)
check "首次发版 CHANGELOG 含首次发布" grep -q '^首次发布。$' "${r}/CHANGELOG.md"
check "首次发版 CHANGELOG 标题" grep -q '^## 0.1.0 (' "${r}/CHANGELOG.md"
git -C "$r" add -A && git -C "$r" commit -qm "chore(release): v0.1.0 [skip ci]"
git -C "$r" tag -a v0.1.0 -m v0.1.0

# 2. tag 之后只有 release 提交 / 不发版类型。
expect "tag 后无新提交" "$r" "release=false version=0.1.0 bump=none"
commit "$r" "docs: 补充说明"
commit "$r" "chore(deps): 升级依赖"
commit "$r" "ci: 调整缓存"
commit "$r" "refactor(engine): 整理调度"
commit "$r" "Feat: 大写类型不算"
commit "$r" "feat 缺少冒号不算"
expect "只有 docs/chore/ci/refactor" "$r" "release=false version=0.1.0 bump=none"

# 3. 0.x：fix → patch，feat → patch，破坏性 → minor。
commit "$r" "fix: 修复多进程不结束"
expect "0.x fix" "$r" "release=true version=0.1.1 bump=patch"
commit "$r" "feat(ui): 新增 中文 描述"
expect "0.x feat" "$r" "release=true version=0.1.1 bump=patch"
r2="$(new_repo pre-major-breaking)"
git -C "$r2" tag -a v0.1.0 -m v0.1.0
commit "$r2" "feat!: 去掉旧接口"
expect "0.x feat!" "$r2" "release=true version=0.2.0 bump=minor"
r3="$(new_repo pre-major-footer)"
git -C "$r3" tag -a v0.1.0 -m v0.1.0
commit "$r3" "refactor: 改写存储" "BREAKING CHANGE: 表结构不兼容"
expect "0.x BREAKING CHANGE 脚注" "$r3" "release=true version=0.2.0 bump=minor"
r4="$(new_repo perf)"
git -C "$r4" tag -a v0.1.0 -m v0.1.0
commit "$r4" "perf(engine): 减少分配"
expect "perf → patch" "$r4" "release=true version=0.1.1 bump=patch"

# 4. 1.x 起按标准 semver。
r5="$(new_repo post-major)"
git -C "$r5" tag -a v1.2.3 -m v1.2.3
commit "$r5" "fix: a"
expect "1.x fix" "$r5" "release=true version=1.2.4 bump=patch"
commit "$r5" "feat: b"
expect "1.x feat" "$r5" "release=true version=1.3.0 bump=minor"
commit "$r5" "fix(api)!: c"
expect "1.x fix!" "$r5" "release=true version=2.0.0 bump=major"

# 5. apply 改写全部版本文件并把新段落置顶写入 CHANGELOG。
(cd "$r" && "$SCRIPT" plan >/dev/null && "$SCRIPT" apply 0.1.1)
check "Cargo.toml workspace 版本" grep -q '^version = "0.1.1"$' "${r}/Cargo.toml"
check "Cargo.toml 依赖版本 ×4" test "$(grep -c '^x-loadgen-[a-z]* = { version = "0.1.1"' "${r}/Cargo.toml")" -eq 4
check "Cargo.lock ×5" test "$(grep -A1 -E '^name = "x-loadgen(-[a-z]+)?"$' "${r}/Cargo.lock" | grep -c '^version = "0.1.1"')" -eq 5
check "package.json" grep -q '^  "version": "0.1.1",$' "${r}/x-loadgen-bin/x-loadgen/web/package.json"
check "openapi.json" grep -q '^    "version": "0.1.1",$' "${r}/x-loadgen-bin/x-loadgen/openapi.json"
check ".env.example" grep -q '^X_LOADGEN_VERSION=0.1.1$' "${r}/deploy/config/.env.example"
check "deploy/README.md" grep -qF "| \`X_LOADGEN_VERSION\` | \`0.1.1\`" "${r}/deploy/README.md"
check "CHANGELOG 新段落在前" test "$(grep -m1 '^## ' "${r}/CHANGELOG.md" | cut -c1-9)" = "## 0.1.1 "
check "CHANGELOG 保留旧段落" grep -q '^## 0.1.0 (' "${r}/CHANGELOG.md"
check "CHANGELOG 只有一个标题" test "$(grep -c '^# Changelog$' "${r}/CHANGELOG.md")" -eq 1
check "CHANGELOG Features 带 scope" grep -q '^\* \*\*ui:\*\* 新增 中文 描述 (' "${r}/CHANGELOG.md"
check "CHANGELOG Bug Fixes" grep -q '^\* 修复多进程不结束 (' "${r}/CHANGELOG.md"
check "CHANGELOG 不收 docs/refactor" bash -c "! grep -q '整理调度\|补充说明' '${r}/CHANGELOG.md'"
check "非法版本被拒绝" bash -c "cd '$r' && ! '$SCRIPT' apply 1.2 2>/dev/null"

# 6. 在 Actions 中（设置了 GITHUB_REPOSITORY）标题带 compare 链接，条目带提交链接。
r6="$(new_repo linked)"
git -C "$r6" tag -a v0.1.0 -m v0.1.0
commit "$r6" "fix: 链接格式"
(cd "$r6" && GITHUB_REPOSITORY=jjdufu/x-loadgen "$SCRIPT" plan >/dev/null)
check "Actions 下标题带 compare 链接" grep -q '^## \[0.1.1\](https://github.com/jjdufu/x-loadgen/compare/v0.1.0...v0.1.1) (' "${r6}/.git/release-notes.md"
check "Actions 下条目带提交链接" grep -q '^\* 链接格式 (\[[0-9a-f]\{7\}\](https://github.com/jjdufu/x-loadgen/commit/[0-9a-f]\{40\}))$' "${r6}/.git/release-notes.md"
(cd "$r6" && out="$(mktemp)" && GITHUB_OUTPUT="$out" "$SCRIPT" plan >/dev/null && grep -qx 'version=0.1.1' "$out" && rm -f "$out")
check "GITHUB_OUTPUT 写入结果" test "$?" -eq 0

# 7. release 提交本身不触发下一次发版。
git -C "$r" add -A && git -C "$r" commit -qm "chore(release): v0.1.1 [skip ci]"
git -C "$r" tag -a v0.1.1 -m v0.1.1
expect "release 提交之后" "$r" "release=false version=0.1.1 bump=none"

echo "${CASES} cases, ${FAILS} failed"
[ "$FAILS" -eq 0 ]
