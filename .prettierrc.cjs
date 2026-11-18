// .prettierrc.cjs
// @see: https://www.prettier.cn

module.exports = {
  // 使 Prettier 自动格式化 Tailwind CSS 类的顺序
  plugins: ["prettier-plugin-tailwindcss", "prettier-plugin-packagejson"],
  // 指定最大换行长度(HTML 和 Vue 模板中的元素)
  printWidth: 150,
  // 强制文本换行(Markdown 和纯文本内容)
  proseWrap: "always",
  // 缩进制表符宽度 | 空格数
  tabWidth: 2,
  // 使用制表符而不是空格缩进行 (true：制表符，false：空格)
  useTabs: false,
  // 结尾不用分号 (true：有，false：没有)
  semi: false,
  // 使用单引号 (true：单引号，false：双引号)
  singleQuote: false,
  // 在对象字面量中决定是否将属性名用引号括起来 可选值 "<as-needed|consistent|preserve>"
  quoteProps: "as-needed",
  // 在JSX中使用单引号而不是双引号 (true：单引号，false：双引号)
  jsxSingleQuote: true,
  // 多行时尽可能打印尾随逗号 "<none|es5|all>"
  trailingComma: "es5",
  // 在对象，数组括号与文字之间加空格 "{ foo: bar }" (true：有，false：没有)
  bracketSpacing: true,
  // 将 > 多行元素放在最后一行的末尾，而不是单独放在下一行
  bracketSameLine: false,
  // 指定要使用的解析器，不需要写文件开头的 @prettier
  requirePragma: false,
  // 可以在文件顶部插入一个特殊标记，指定该文件已使用 Prettier 格式化
  insertPragma: false,
  // 在html中空格是否是敏感的
  htmlWhitespaceSensitivity: "css",
  // 控制在 Vue 单文件组件中 <script> 和 <style scoped> 标签内的代码缩进方式
  vueIndentScriptAndStyle: false,
  // 换行符
  endOfLine: "auto",
  rangeStart: 0,
  rangeEnd: Infinity,
}
