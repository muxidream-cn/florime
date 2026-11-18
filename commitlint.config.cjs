// @see https://commitlint.js.org/
// @see https://cz-git.qbb.sh/zh/guide/
const { defineConfig } = require("cz-git")
const { resolve } = require("path")
const fs = require("fs")

// 项目配置的默认 scope
const defaultScopeArr = ["core", "deps", "config", "ui", "monitor", "win", "db", "api", "store", "hook", "util", "vite", "readme"]

/** 本地缓存文件 */
const __SCOPE_CACHE_PATH = resolve(__dirname, "./.git/scope-cache.json")

/** 用正则过滤有效的 scope */
const isValidScope = (scope) => {
  const regex = /^[a-zA-Z0-9\u4e00-\u9fa5_-]+$/
  return regex.test(scope)
}

/** 将自定义 scope 写入缓存 */
const setCacheScope = (scope) => {
  if (!scope) return

  const localScope = defaultScopeArr.map((s) => s.trim().toLowerCase())
  const scopes = scope
    .toLowerCase()
    .split(",")
    .map((s) => s.trim())
    .filter(isValidScope)
    .filter((s) => !localScope.includes(s))

  if (scopes.length === 0) return

  let cacheScopes = []
  if (fs.existsSync(__SCOPE_CACHE_PATH)) {
    cacheScopes = JSON.parse(fs.readFileSync(__SCOPE_CACHE_PATH, "utf8"))
  }

  const newScopes = new Set([...cacheScopes, ...scopes])
  fs.writeFileSync(__SCOPE_CACHE_PATH, JSON.stringify([...newScopes], null, 2), "utf8")
}

/** 获取缓存的 scope 并与项目的 scope 合并 */
const getCacheScope = () => {
  if (!fs.existsSync(__SCOPE_CACHE_PATH)) {
    return defaultScopeArr
  }
  const cacheScopeArr = JSON.parse(fs.readFileSync(__SCOPE_CACHE_PATH, "utf8"))
  return [...new Set([...defaultScopeArr, ...cacheScopeArr])]
}

module.exports = defineConfig({
  extends: ["@commitlint/config-conventional"],
  parserPreset: {
    parserOpts: {
      headerPattern: /^([\w-]+)(?:\(([\w\u4e00-\u9fa5a-zA-Z0-9,_-]+)\))?:\s([\s\S]+)$/,
      headerCorrespondence: ["type", "scope", "subject"],
    },
  },
  rules: {
    "scope-case": [2, "always", ["lower-case", "camel-case", "kebab-case"]],
    "subject-case": [0],
    "type-empty": [2, "never"],
    "subject-empty": [2, "never"],
    "type-enum": [2, "always", ["init", "feat", "fix", "docs", "style", "refactor", "perf", "test", "build", "ci", "chore", "revert", "release"]],
  },
  prompt: {
    alias: { fd: "docs: fix typos" },
    messages: {
      type: "选择你要提交的类型 :",
      scope: "选择一个提交范围（可为空，可自定义，空格键或右键可多选，回车键确认）:",
      customScope: "请输入自定义的提交范围（仅支持用下划线、中文字符、小写英文字符、数字，且用 , 分隔）:",
      subject: "填写简短精炼的变更描述 :\n",
      body: "填写更加详细的变更描述（可选）。使用 | 换行 :\n",
      breaking: "列举非兼容性重大的变更（可选）。使用 | 换行 :\n",
      customFooterPrefix: "输入自定义issue前缀 :",
      footer: "列举关联issue (可选) 例如: #31, #I3244 :\n",
      confirmCommit: "是否提交或修改commit ?",
    },
    types: [
      { value: "init", name: "🛠️  init:      初始化 | Project initialization", emoji: ":hammer_and_wrench:" },
      { value: "feat", name: "✨ feat:      新增功能 | A new feature", emoji: ":sparkles:" },
      { value: "fix", name: "🐛 fix:       修复缺陷 | A bug fix", emoji: ":bug:" },
      { value: "docs", name: "📖 docs:      文档更新 | Documentation only changes", emoji: ":book:" },
      {
        value: "style",
        name: "💅 style:     代码格式（不影响功能，例如空格、分号等格式修正） | Changes that do not affect the meaning of the code",
        emoji: ":nail_care:",
      },
      {
        value: "refactor",
        name: "🔨 refactor:  代码重构（不包括 bug 修复、功能新增） | A code change that neither fixes a bug nor adds a feature",
        emoji: ":wrench:",
      },
      { value: "perf", name: "🚀 perf:      性能优化 | A code change that improves performance", emoji: ":rocket:" },
      {
        value: "test",
        name: "🧪 test:      测试相关，添加疏漏测试或已有测试改动 | Adding missing tests or correcting existing tests",
        emoji: ":test_tube:",
      },
      {
        value: "build",
        name: "📦 build:     构建流程、外部依赖变更（如升级 npm 包、修改 webpack 配置等） | Changes that affect the build system or external dependencies",
        emoji: ":package:",
      },
      {
        value: "ci",
        name: "🤖 ci:        持续集成相关的提交（修改 CI 配置文件、构建流程、自动化测试等） | Changes to our CI configuration files and scripts",
        emoji: ":robot:",
      },
      {
        value: "chore",
        name: "🔧 chore:     对构建过程或辅助工具和库的更改（不影响源文件、测试用例） | Other changes that do not modify src or test files",
        emoji: ":wrench:",
      },
      { value: "revert", name: "⏪ revert:    回退提交的代码 | Revert to a commit", emoji: ":rewind:" },
      {
        value: "release",
        name: "🎉 release:   版本发布 | Version release",
        emoji: ":tada:",
      },
    ],
    useEmoji: true,
    emojiAlign: "center",
    useAI: false,
    aiNumber: 1,
    themeColorCode: "",
    scopes: getCacheScope(),
    formatMessageCB: ({ defaultMessage, scope }) => {
      // eslint-disable-next-line no-control-regex
      setCacheScope(scope.replaceAll(/\x1B\[[0-9;]*[mG]/g, ""))
      return defaultMessage
    },
    allowCustomScopes: true,
    allowEmptyScopes: true,
    customScopesAlign: "bottom",
    customScopesAlias: "自定义",
    emptyScopesAlias: "空",
    upperCaseSubject: false,
    markBreakingChangeMode: false,
    allowBreakingChanges: ["feat", "fix"],
    breaklineNumber: 100,
    breaklineChar: "|",
    skipQuestions: [],
    customIssuePrefixAlign: "top",
    emptyIssuePrefixAlias: "skip",
    customIssuePrefixAlias: "custom",
    allowCustomIssuePrefix: true,
    allowEmptyIssuePrefix: true,
    confirmColorize: true,
    scopeOverrides: undefined,
    defaultBody: "",
    defaultIssues: "",
    defaultScope: "",
    defaultSubject: "",
    enableMultipleScopes: true,
  },
})
