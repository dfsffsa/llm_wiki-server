/**
 * i18n.js — 站内国际化：唯一文案源 + 运行时翻译引擎
 * ====================================================
 *
 * 本文件是项目"唯一文案源"：所有公开页面（landing / auth / lite）的用户
 * 可见文字都集中在这里的 COPY 字典。页面 HTML 不写任何文案，只用
 * data-i18n 属性引用 key：
 *
 *   <span data-i18n="hero.cta.start"></span>          → 文本
 *   <input data-i18n-placeholder="lite.search.placeholder">
 *   <a data-i18n-title="...">                          → title
 *   <div data-i18n-html="...">                         → innerHTML（含链接/<br>）
 *   <meta data-i18n-meta-content="home.metaDesc">      → meta description
 *
 * 改文案（换品牌名 / slogan / 按钮文字）= 改本文件对应 key 的 zh / en 两行，
 * 页面无需改动，刷新即生效。
 *
 * 语言检测优先级：?lang= > localStorage.lang > navigator.language
 * （zh* 开头的浏览器 → 中文，其余 → 英文）。
 *
 * 服务端 Rust 侧消息不在此翻译（保持中文，避免与 billing 对 auth 的改动
 * 冲突），而是按 error.code 用 ERRORS 字典本地化；未知 code 回退服务端
 * 原始消息（I18N.localizeServerError(code, fallback)）。
 *
 * 加载方式：每页 <script src="/i18n.js" defer></script>，defer 保证在 DOM
 * 解析完成后、内联脚本执行后运行，避免翻译覆盖内联脚本设置的内容。
 */
(function () {
  'use strict';

  if (window.I18N) return; // 防止重复加载

  var STORAGE_KEY = 'lang';

  // ─── 文案字典（唯一文案源）───────────────────────────────────────────
  // 每条 key 含 zh + en 两版，相邻排列、不易漏改。
  var COPY = {
    // 品牌
    'brand.name':    { zh: 'DocuChat', en: 'DocuChat' },
    // 品牌 slogan（landing Hero 标题）：换 slogan 只改这一行，中英两版相邻
    'brand.tagline': {
      zh: '让知识库<span class="text-primary-600">为你工作</span>',
      en: 'Make your knowledge base <span class="text-primary-600">work for you</span>',
    },

    // ── landing：导航 ──
    'nav.features': { zh: '功能', en: 'Features' },
    'nav.pricing':  { zh: '定价', en: 'Pricing' },
    'nav.help':     { zh: '帮助', en: 'Help' },
    'nav.login':    { zh: '登录', en: 'Log in' },
    'nav.trial':    { zh: '免费试用', en: 'Start free' },

    // ── landing：Hero ──
    'home.metaDesc': {
      zh: 'DocuChat是AI驱动的智能知识库问答平台，让育儿知识和职场文档触手可及。',
      en: 'DocuChat is an AI-powered Q&A platform for your knowledge base, putting parenting and career knowledge at your fingertips.',
    },
    'hero.subtitle': {
      zh: 'AI驱动的智能问答，育儿知识和职场文档触手可及。<br>上传文档，立即获得精准回答。',
      en: 'AI-powered Q&A that puts parenting and career knowledge at your fingertips.<br>Upload documents and get precise answers instantly.',
    },
    'hero.cta.start':   { zh: '免费开始', en: 'Get started free' },
    'hero.cta.pricing': { zh: '查看定价', en: 'View pricing' },
    'hero.screenshot':  { zh: '产品截图展示区', en: 'Product preview' },

    // ── landing：用户场景 ──
    'use-cases.title':    { zh: '为谁设计？', en: 'Who is it for?' },
    'use-cases.subtitle': {
      zh: '无论你是育儿达人还是职场精英，DocuChat都能帮你高效管理知识',
      en: 'Whether you are a parent or a working professional, DocuChat helps you manage knowledge efficiently',
    },
    'use-cases.tab.parenting': { zh: '👶 育儿人士', en: '👶 Parents' },
    'use-cases.tab.work':      { zh: '💼 职场人士', en: '💼 Professionals' },
    'use-cases.parenting.kb.title': { zh: '育儿知识库', en: 'Parenting Knowledge Base' },
    'use-cases.parenting.kb.desc': {
      zh: '整合育儿书籍、专家建议、经验分享，一个问题找到所有答案。',
      en: 'Consolidates parenting books, expert advice and shared experience — one question, all the answers.',
    },
    'use-cases.parenting.edu.title': { zh: '教育方法', en: 'Education Methods' },
    'use-cases.parenting.edu.desc': {
      zh: '蒙特梭利、瑞吉欧、华德福...快速检索适合你家的教育理念。',
      en: 'Montessori, Reggio, Waldorf… quickly find the educational approach that fits your family.',
    },
    'use-cases.parenting.health.title': { zh: '健康护理', en: 'Health & Care' },
    'use-cases.parenting.health.desc': {
      zh: '儿童常见病、疫苗接种、营养搭配，专业信息触手可及。',
      en: 'Common childhood illnesses, vaccinations, nutrition — professional information at your fingertips.',
    },
    'use-cases.work.docs.title': { zh: '项目文档', en: 'Project Documents' },
    'use-cases.work.docs.desc': {
      zh: '需求文档、会议记录、技术方案，团队知识一键检索。',
      en: 'Requirement docs, meeting notes, technical plans — search team knowledge in one click.',
    },
    'use-cases.work.notes.title': { zh: '技能笔记', en: 'Skill Notes' },
    'use-cases.work.notes.desc': {
      zh: '学习笔记、最佳实践、代码片段，打造个人知识体系。',
      en: 'Study notes, best practices, code snippets — build your personal knowledge system.',
    },
    'use-cases.work.flow.title': { zh: '工作流程', en: 'Workflows' },
    'use-cases.work.flow.desc': {
      zh: 'SOP、操作手册、常见问题，新员工快速上手。',
      en: 'SOPs, runbooks, FAQs — get new team members up to speed fast.',
    },

    // ── landing：核心功能 ──
    'features.title':    { zh: '核心功能', en: 'Core Features' },
    'features.subtitle': { zh: '强大而简洁，让知识管理变得轻松', en: 'Powerful yet simple — makes knowledge management effortless' },
    'features.qa.title':     { zh: 'AI智能问答', en: 'AI Q&A' },
    'features.qa.desc':      { zh: '自然语言提问，AI理解上下文，给出精准回答。', en: 'Ask in natural language — AI understands context and answers precisely.' },
    'features.search.title': { zh: '全文搜索', en: 'Full-text Search' },
    'features.search.desc':  { zh: '关键词检索整个知识库，毫秒级响应。', en: 'Search the entire knowledge base by keyword with millisecond response.' },
    'features.graph.title':  { zh: '知识图谱', en: 'Knowledge Graph' },
    'features.graph.desc':   { zh: '可视化文档关联，发现隐藏的知识网络。', en: 'Visualize document links and discover hidden knowledge networks.' },
    'features.security.title': { zh: '安全私密', en: 'Secure & Private' },
    'features.security.desc':  { zh: '数据加密存储，隐私保护，只有你能访问。', en: 'Encrypted storage with privacy protection — only you can access it.' },

    // ── landing：三步开始 ──
    'steps.title':    { zh: '三步开始', en: 'Get Started in 3 Steps' },
    'steps.subtitle': { zh: '简单几步，即可拥有自己的AI知识库', en: 'A few simple steps to your own AI knowledge base' },
    'steps.1.title': { zh: '上传文档', en: 'Upload Documents' },
    'steps.1.desc':  { zh: '支持PDF、Word、Markdown等多种格式', en: 'Supports PDF, Word, Markdown and more' },
    'steps.2.title': { zh: 'AI自动整理', en: 'AI Organizes Automatically' },
    'steps.2.desc':  { zh: 'AI分析内容，建立索引，生成知识图谱', en: 'AI analyzes content, builds indexes and generates knowledge graphs' },
    'steps.3.title': { zh: '智能问答', en: 'Smart Q&A' },
    'steps.3.desc':  { zh: '用自然语言提问，获取精准回答', en: 'Ask in natural language and get precise answers' },

    // ── landing：定价 ──
    'pricing.title':    { zh: '简单透明的定价', en: 'Simple, Transparent Pricing' },
    'pricing.subtitle': { zh: '选择适合你的方案', en: 'Choose the plan that fits you' },
    'pricing.free.name':   { zh: '免费版', en: 'Free' },
    'pricing.free.period': { zh: '/永久', en: '/forever' },
    'pricing.free.questions': { zh: '每天3个AI问题', en: '3 AI questions per day' },
    'pricing.free.search':    { zh: '全文搜索', en: 'Full-text search' },
    'pricing.free.graph':     { zh: '知识图谱', en: 'Knowledge graph' },
    'pricing.free.cta':       { zh: '免费开始', en: 'Start free' },
    'pricing.pro.badge':      { zh: '推荐', en: 'Recommended' },
    'pricing.pro.name':       { zh: '专业版', en: 'Pro' },
    'pricing.pro.period':     { zh: '/月', en: '/month' },
    'pricing.pro.unlimited':  { zh: '无限AI问题', en: 'Unlimited AI questions' },
    'pricing.pro.advancedSearch': { zh: '高级搜索', en: 'Advanced search' },
    'pricing.pro.priority':   { zh: '优先支持', en: 'Priority support' },
    'pricing.pro.export':     { zh: '导出功能', en: 'Export' },
    'pricing.pro.cta':        { zh: '立即订阅', en: 'Subscribe now' },
    'pricing.fullCompare':    { zh: '查看完整功能对比 →', en: 'View full feature comparison →' },

    // ── landing：FAQ ──
    'faq.title':   { zh: '常见问题', en: 'FAQ' },
    'faq.1.q':     { zh: '免费版有什么限制？', en: 'What are the limits of the free plan?' },
    'faq.1.a': {
      zh: '免费版每天可以提出3个AI问题，支持全文搜索和知识图谱功能。如果需要更多问题次数或高级功能，可以升级到专业版。',
      en: 'The free plan allows 3 AI questions per day with full-text search and knowledge graph. Upgrade to Pro for more questions and advanced features.',
    },
    'faq.2.q': { zh: '可以随时取消订阅吗？', en: 'Can I cancel my subscription anytime?' },
    'faq.2.a': {
      zh: '可以，随时可以在账户设置中取消订阅。取消后，当前订阅周期仍有效，到期后自动降级为免费版。',
      en: 'Yes — cancel anytime from account settings. Your current billing period stays active, then you automatically downgrade to the free plan.',
    },
    'faq.3.q': { zh: '支持哪些文档格式？', en: 'What document formats are supported?' },
    'faq.3.a': {
      zh: '支持PDF、Word文档、Markdown、纯文本等常见格式。未来会支持更多格式。',
      en: 'PDF, Word, Markdown, plain text and other common formats. More coming soon.',
    },
    'faq.4.q': { zh: '我的数据安全吗？', en: 'Is my data safe?' },
    'faq.4.a': {
      zh: '完全安全。数据加密存储在服务器上，只有您本人可以访问。我们不会与第三方共享您的数据。',
      en: 'Absolutely. Data is encrypted on our servers and only you can access it. We never share your data with third parties.',
    },
    'faq.5.q': { zh: '有团队版或企业版吗？', en: 'Do you offer a team or enterprise plan?' },
    'faq.5.a': {
      zh: '目前提供个人版，团队版正在开发中。如有企业需求，请联系我们获取定制方案。',
      en: 'We currently offer individual plans; team plans are in development. For enterprise needs, contact us for a custom solution.',
    },

    // ── landing：CTA + 页脚 ──
    'cta.title':    { zh: '准备好开始了吗？', en: 'Ready to get started?' },
    'cta.subtitle': { zh: '免费注册，立即体验AI知识库的强大功能', en: 'Sign up free and experience the power of an AI knowledge base' },
    'cta.button':   { zh: '免费开始', en: 'Get started free' },
    'footer.desc':  { zh: 'AI驱动的智能知识库问答平台', en: 'AI-powered Q&A platform for your knowledge base' },
    'footer.tagline': { zh: '让育儿知识和职场文档触手可及', en: 'Parenting and career knowledge at your fingertips' },
    'footer.product':    { zh: '产品', en: 'Product' },
    'footer.product.feat': { zh: '功能', en: 'Features' },
    'footer.product.price':{ zh: '定价', en: 'Pricing' },
    'footer.product.help': { zh: '帮助中心', en: 'Help Center' },
    'footer.company':    { zh: '公司', en: 'Company' },
    'footer.company.about':   { zh: '关于我们', en: 'About us' },
    'footer.company.privacy': { zh: '隐私政策', en: 'Privacy Policy' },
    'footer.company.terms':   { zh: '服务条款', en: 'Terms of Service' },
    'footer.company.contact': { zh: '联系我们', en: 'Contact us' },
    'footer.copyright': { zh: '© 2024 DocuChat. All rights reserved.', en: '© 2024 DocuChat. All rights reserved.' },

    // ── auth：登录页 ──
    'auth.login.welcome':       { zh: '欢迎回到', en: 'Welcome back to' },
    'auth.login.subtitle':      { zh: '登录后开始你的AI知识库之旅', en: 'Log in and start your AI knowledge journey' },
    'auth.login.feat.qa.title': { zh: 'AI智能问答', en: 'AI Q&A' },
    'auth.login.feat.qa.desc':  { zh: '自然语言提问，精准回答', en: 'Ask naturally, get precise answers' },
    'auth.login.feat.search.title': { zh: '全文搜索', en: 'Full-text Search' },
    'auth.login.feat.search.desc':  { zh: '毫秒级检索，快速定位', en: 'Millisecond search, fast results' },
    'auth.login.feat.graph.title':  { zh: '知识图谱', en: 'Knowledge Graph' },
    'auth.login.feat.graph.desc':   { zh: '可视化关联，发现新知', en: 'Visualize connections, discover insights' },
    'auth.login.title':       { zh: '登录账号', en: 'Log in' },
    'auth.login.noAccount':   { zh: '还没有账号？', en: "Don't have an account?" },
    'auth.login.signupLink':  { zh: '免费注册', en: 'Sign up free' },
    'auth.login.verifiedBanner':   { zh: '✅ 邮箱已验证，请登录', en: '✅ Email verified — please log in' },
    'auth.login.verifyFailedBanner': {
      zh: '⚠️ 验证链接无效或已过期，请重新注册或联系管理员',
      en: '⚠️ Verification link is invalid or expired. Please re-register or contact the administrator.',
    },
    'auth.login.email':       { zh: '邮箱地址', en: 'Email address' },
    'auth.login.password':    { zh: '密码', en: 'Password' },
    'auth.login.forgot':      { zh: '忘记密码？', en: 'Forgot password?' },
    'auth.login.button':      { zh: '登录', en: 'Log in' },
    'auth.login.buttonLoading': { zh: '登录中...', en: 'Logging in…' },
    'auth.login.or':          { zh: '或者', en: 'or' },
    'auth.login.wechat':      { zh: '使用微信登录', en: 'Log in with WeChat' },
    'auth.login.agree': {
      zh: '登录即表示你同意我们的 <a href="/terms" class="text-primary-600 hover:underline">服务条款</a> 和 <a href="/privacy" class="text-primary-600 hover:underline">隐私政策</a>',
      en: 'By logging in, you agree to our <a href="/terms" class="text-primary-600 hover:underline">Terms of Service</a> and <a href="/privacy" class="text-primary-600 hover:underline">Privacy Policy</a>',
    },
    'auth.login.err.emailNotVerified': {
      zh: '邮箱未验证，请先查收验证邮件。没收到？<a href="/register" class="underline">重新注册</a>',
      en: 'Email not verified — check your inbox. Did not get it? <a href="/register" class="underline">Register again</a>',
    },
    'auth.login.err.generic': { zh: '登录失败', en: 'Login failed' },

    // ── auth：注册页 ──
    'auth.register.welcome1': { zh: '开始你的', en: 'Begin your' },
    'auth.register.welcome2': { zh: '知识之旅', en: 'knowledge journey' },
    'auth.register.subtitle': { zh: '免费注册，立即体验AI知识库的强大功能', en: 'Sign up free and experience the power of an AI knowledge base' },
    'auth.register.freeIncludes': { zh: '免费版包含：', en: 'The free plan includes:' },
    'auth.register.free.questions': { zh: '每天3个AI问题', en: '3 AI questions per day' },
    'auth.register.free.search':    { zh: '全文搜索', en: 'Full-text search' },
    'auth.register.free.graph':     { zh: '知识图谱', en: 'Knowledge graph' },
    'auth.register.moreFeatures': { zh: '需要更多功能？', en: 'Need more?' },
    'auth.register.viewPro':      { zh: '查看专业版', en: 'View Pro' },
    'auth.register.title':        { zh: '创建账号', en: 'Create account' },
    'auth.register.hasAccount':   { zh: '已有账号？', en: 'Already have an account?' },
    'auth.register.loginLink':    { zh: '立即登录', en: 'Log in' },
    'auth.register.email':        { zh: '邮箱地址', en: 'Email address' },
    'auth.register.password':     { zh: '密码', en: 'Password' },
    'auth.register.passwordPlaceholder': { zh: '至少8位字符', en: 'At least 8 characters' },
    'auth.register.passwordHint': { zh: '密码至少需要8个字符', en: 'Password must be at least 8 characters' },
    'auth.register.confirmPassword':    { zh: '确认密码', en: 'Confirm password' },
    'auth.register.confirmPlaceholder': { zh: '再次输入密码', en: 'Re-enter password' },
    'auth.register.terms': {
      zh: '我已阅读并同意 <a href="/terms" class="text-primary-600 hover:underline">服务条款</a> 和 <a href="/privacy" class="text-primary-600 hover:underline">隐私政策</a>',
      en: 'I have read and agree to the <a href="/terms" class="text-primary-600 hover:underline">Terms of Service</a> and <a href="/privacy" class="text-primary-600 hover:underline">Privacy Policy</a>',
    },
    'auth.register.button':        { zh: '创建账号', en: 'Create account' },
    'auth.register.buttonLoading': { zh: '注册中...', en: 'Registering…' },
    'auth.register.or':            { zh: '或者', en: 'or' },
    'auth.register.wechat':        { zh: '使用微信注册', en: 'Register with WeChat' },
    'auth.register.backToLogin':   { zh: '返回登录', en: 'Back to login' },
    'auth.register.err.passwordMismatch': { zh: '两次输入的密码不一致', en: 'Passwords do not match' },
    'auth.register.err.termsRequired':    { zh: '请同意服务条款和隐私政策', en: 'Please agree to the terms and privacy policy' },
    'auth.register.err.generic':  { zh: '注册失败', en: 'Registration failed' },
    'auth.register.success':      { zh: '验证邮件已发送，请检查邮箱', en: 'Verification email sent — please check your inbox' },

    // ── auth：重置密码页 ──
    'auth.reset.title':     { zh: '重置密码', en: 'Reset Password' },
    'auth.reset.step1Hint': {
      zh: '输入邮箱,我们会向你的邮箱发送重置 Token(若邮箱已注册)。',
      en: 'Enter your email and we will send a reset token (if the email is registered).',
    },
    'auth.reset.email':       { zh: '邮箱', en: 'Email' },
    'auth.reset.step1Button': { zh: '发送重置链接', en: 'Send reset link' },
    'auth.reset.step2Hint': {
      zh: '粘贴邮件中的 token,然后输入新密码。',
      en: 'Paste the token from the email, then enter a new password.',
    },
    'auth.reset.token':        { zh: 'Token', en: 'Token' },
    'auth.reset.newPassword':  { zh: '新密码', en: 'New password' },
    'auth.reset.step2Button':  { zh: '设置新密码', en: 'Set new password' },
    'auth.reset.backToLogin':  { zh: '返回登录', en: 'Back to login' },
    'auth.reset.msg.sent':     { zh: '若该邮箱已注册,你将收到一封带 token 的邮件', en: 'If that email is registered, you will receive an email with a token' },
    'auth.reset.msg.failed':   { zh: '请求失败', en: 'Request failed' },
    'auth.reset.msg.resetFailed': { zh: '重置失败', en: 'Reset failed' },

    // ── lite 页 ──
    'lite.empty.title':      { zh: '有什么可以帮你？', en: 'How can I help?' },
    'lite.composer.placeholder': { zh: '输入你的问题…', en: 'Type your question…' },
    'lite.composer.replying':    { zh: '正在回复中，请稍候…', en: 'Replying, please wait…' },
    'lite.search.placeholder':   { zh: '搜索当前知识库…', en: 'Search this knowledge base…' },
    'lite.sidebar.knowledge': { zh: '知识库', en: 'Knowledge Base' },
    'lite.sidebar.search':    { zh: '🔍 搜索对话', en: '🔍 Search chats' },
    'lite.sidebar.new':       { zh: '＋ 新建', en: '＋ New' },
    'lite.sidebar.logout':    { zh: '登出', en: 'Log out' },
    'lite.usage.remaining':   { zh: '今日剩余 {used}/{limit}', en: '{used}/{limit} left today' },
    'lite.menu.aria':         { zh: '菜单', en: 'Menu' },
    'lite.send.aria':         { zh: '发送', en: 'Send' },
    'lite.searchBack.aria':   { zh: '返回聊天', en: 'Back to chat' },
    'lite.metaDesc':          { zh: '选一个话题，向知识库提问', en: 'Pick a topic and ask the knowledge base' },
    'lite.searchStatus.searching': { zh: '正在搜索…', en: 'Searching…' },
    'lite.searchStatus.empty':     { zh: '未找到相关内容', en: 'No results found' },
    'lite.searchStatus.failed':    { zh: '搜索请求失败', en: 'Search request failed' },
    'lite.reasoning.title':        { zh: '思考过程', en: 'Thinking process' },
    'lite.streamStatus.replying':  { zh: '正在回复…', en: 'Replying…' },
    'lite.streamStatus.searching': { zh: '正在检索资料…', en: 'Searching the knowledge base…' },
    'lite.streamStatus.thinking':  { zh: '正在思考…', en: 'Thinking…' },
    'lite.streamStatus.generating':{ zh: '正在生成回答…', en: 'Generating answer…' },
    'lite.sources.label':     { zh: '📎 参考来源', en: '📎 Sources' },
    'lite.sourceCard.view':   { zh: '查看原文 →', en: 'View original →' },
    'lite.history.delete':    { zh: '删除', en: 'Delete' },
    'lite.chat.timeout':      { zh: '回复超时，请稍后重试。', en: 'Reply timed out, please try again.' },
    'lite.chat.noReply':      { zh: '（无回复内容）', en: '(No reply)' },
    'lite.chat.quotaAlert':   { zh: '今日额度已用完，明日重置', en: 'Daily quota reached — resets tomorrow' },
    'lite.banner.chatDisabled': {
      zh: '问答功能暂不可用，请检查服务端 LLM 配置。',
      en: 'Chat is temporarily unavailable — check the server LLM configuration.',
    },
    'lite.banner.noProjects': { zh: '暂无可用知识库，请在服务端配置 projects。', en: 'No knowledge bases available — configure projects on the server.' },
    'lite.banner.connection': { zh: '无法连接服务：{msg}', en: 'Cannot connect to server: {msg}' },
    'lite.systemPrompt.role': {
      zh: '你是「{title}」知识库助手，用简洁中文回答家长/职场新人的实际问题。',
      en: 'You are the assistant for the "{title}" knowledge base. Answer practical questions for parents and professionals concisely in English.',
    },
    'lite.systemPrompt.noContext': {
      zh: '优先依据下方检索到的资料；若无相关资料，请诚实说明并给出通用建议。',
      en: 'Prioritize the retrieved sources below; if none are relevant, say so honestly and give general advice.',
    },
    'lite.systemPrompt.cite': {
      zh: '回答时若引用具体资料，请在引用处标注对应的编号 [1][2] 等。',
      en: 'When citing specific sources in your answer, mark them with the corresponding numbers like [1][2].',
    },
    'lite.systemPrompt.contextLabel': { zh: '\n--- 检索资料 ---\n', en: '\n--- Retrieved sources ---\n' },
  };

  // ─── 服务端错误码本地化字典（按 error.code）─────────────────────────
  // 对应 overlay/auth/src/error.rs 的 AuthError::code()。
  var ERRORS = {
    'invalid_input':        { zh: '输入有误，请检查后重试', en: 'Invalid input, please check and try again' },
    'email_already_exists': { zh: '该邮箱已注册', en: 'This email is already registered' },
    'invalid_credentials':  { zh: '邮箱或密码错误', en: 'Incorrect email or password' },
    'not_authenticated':    { zh: '请先登录', en: 'Please log in first' },
    'rate_limited':         { zh: '尝试过于频繁，请稍后再试', en: 'Too many attempts, please try again later' },
    'daily_limit_exceeded': { zh: '今日额度已用完，明日重置', en: 'Daily quota reached — resets tomorrow' },
    'invalid_reset_token':  { zh: '重置链接无效', en: 'Invalid reset link' },
    'expired_reset_token':  { zh: '重置链接已过期', en: 'Reset link has expired' },
    'email_not_verified':   { zh: '邮箱未验证，请先查收验证邮件', en: 'Email not verified, please check your inbox' },
    'email_already_verified': { zh: '邮箱已验证', en: 'Email already verified' },
    'email_change_conflict':  { zh: '邮箱变更冲突', en: 'Email change conflict' },
    'internal_error':       { zh: '服务内部错误', en: 'Internal server error' },
    'registration_closed':  { zh: '注册已关闭，请联系管理员开通账号', en: 'Registration is closed, please contact the administrator' },
    'not_found':            { zh: '未找到', en: 'Not found' },
  };

  // ─── 页面标题（pageTitle.<page> 只存页名部分，品牌名由 JS 拼接）──────
  // home 页标题 = "DocuChat - <suffix>"，其余 = "<label> - DocuChat"。
  var PAGE_TITLE = {
    home:     { zh: 'AI智能知识库问答', en: 'AI-powered Q&A for your knowledge base' },
    login:    { zh: '登录', en: 'Log in' },
    register: { zh: '注册', en: 'Sign up' },
    reset:    { zh: '重置密码', en: 'Reset password' },
    lite:     { zh: '知识问答', en: 'Knowledge Q&A' },
  };

  // ─── 语言状态 ────────────────────────────────────────────────────────
  var LANG = 'en';

  function detectLang() {
    // 1) ?lang=zh|en
    try {
      var q = new URLSearchParams(window.location.search).get('lang');
      if (q === 'zh' || q === 'en') return { lang: q, persist: true };
    } catch (e) { /* ignore */ }
    // 2) localStorage.lang
    try {
      var saved = localStorage.getItem(STORAGE_KEY);
      if (saved === 'zh' || saved === 'en') return { lang: saved, persist: false };
    } catch (e) { /* ignore */ }
    // 3) navigator.language（zh* → 中文，其余 → 英文）
    var nav = (navigator.language || '').toLowerCase();
    return { lang: nav.indexOf('zh') === 0 ? 'zh' : 'en', persist: false };
  }

  // ─── 取文案 ──────────────────────────────────────────────────────────
  function t(key) {
    var e = COPY[key];
    if (!e) return key; // 缺 key 时原样返回，便于发现漏配
    return e[LANG] != null ? e[LANG] : e.zh;
  }

  /** 带占位符的文案：tpl('lite.usage.remaining', { used: 3, limit: 50 }) */
  function tpl(key, params) {
    var s = t(key);
    if (!params) return s;
    return s.replace(/\{(\w+)\}/g, function (m, k) {
      return params[k] != null ? String(params[k]) : m;
    });
  }

  /** 服务端错误码 → 本地化文本；未知 code 返回 null（由调用方回退）。 */
  function errorText(code) {
    if (!code) return null;
    var e = ERRORS[code];
    if (e) return e[LANG] != null ? e[LANG] : e.zh;
    return null;
  }

  /** 本地化服务端错误：有 errors[code] 用之，否则回退 fallback。 */
  function localizeServerError(code, fallback) {
    var s = errorText(code);
    return s != null ? s : (fallback || code || '');
  }

  // ─── 页面识别 + 标题 ─────────────────────────────────────────────────
  function currentPage() {
    var p = window.location.pathname;
    if (p === '/login') return 'login';
    if (p === '/register') return 'register';
    if (p === '/reset-password') return 'reset';
    if (p === '/lite/' || p === '/lite' || p.indexOf('/lite/') === 0) return 'lite';
    return 'home';
  }

  function setDocumentTitle() {
    var page = currentPage();
    var item = PAGE_TITLE[page];
    if (!item) return;
    var part = item[LANG] || item.zh;
    var brand = t('brand.name');
    document.title = page === 'home' ? (brand + ' - ' + part) : (part + ' - ' + brand);
  }

  // ─── 切换按钮 ────────────────────────────────────────────────────────
  var BTN_STYLE =
    'display:inline-flex;align-items:center;gap:4px;' +
    'background:var(--surface,#ffffff);color:var(--text,#1f2937);' +
    'border:1px solid var(--border,#d1d5db);border-radius:9999px;' +
    'padding:4px 12px;font-size:13px;line-height:1.5;cursor:pointer;' +
    'box-shadow:0 1px 3px rgba(0,0,0,.12);white-space:nowrap;';

  function renderToggle() {
    var label = LANG === 'zh' ? 'Switch to English' : '切换到中文';
    var text = '🌐 ' + (LANG === 'zh' ? 'EN' : '中');
    var slots = document.querySelectorAll('[data-i18n-toggle]');
    if (slots.length) {
      slots.forEach(function (slot) {
        slot.innerHTML = '';
        var b = document.createElement('button');
        b.type = 'button';
        b.textContent = text;
        b.setAttribute('aria-label', label);
        b.title = label;
        b.style.cssText = BTN_STYLE;
        b.addEventListener('click', function () { toggle(); });
        slot.appendChild(b);
      });
      return;
    }
    // 无 slot 时兜底：右下角悬浮按钮
    var btn = window.__i18nFloatingBtn;
    if (!btn) {
      btn = document.createElement('button');
      btn.type = 'button';
      btn.style.cssText = BTN_STYLE +
        'position:fixed;right:1rem;bottom:1rem;z-index:9999;' +
        'padding:6px 14px;';
      btn.addEventListener('click', function () { toggle(); });
      document.body.appendChild(btn);
      window.__i18nFloatingBtn = btn;
    }
    btn.textContent = text;
    btn.setAttribute('aria-label', label);
    btn.title = label;
  }

  // ─── 应用翻译 ────────────────────────────────────────────────────────
  function applyTranslations() {
    document.documentElement.lang = LANG === 'zh' ? 'zh-CN' : 'en';
    document.querySelectorAll('[data-i18n]').forEach(function (el) {
      el.textContent = t(el.getAttribute('data-i18n'));
    });
    document.querySelectorAll('[data-i18n-placeholder]').forEach(function (el) {
      el.setAttribute('placeholder', t(el.getAttribute('data-i18n-placeholder')));
    });
    document.querySelectorAll('[data-i18n-title]').forEach(function (el) {
      el.setAttribute('title', t(el.getAttribute('data-i18n-title')));
    });
    document.querySelectorAll('[data-i18n-aria-label]').forEach(function (el) {
      el.setAttribute('aria-label', t(el.getAttribute('data-i18n-aria-label')));
    });
    document.querySelectorAll('[data-i18n-html]').forEach(function (el) {
      el.innerHTML = t(el.getAttribute('data-i18n-html'));
    });
    document.querySelectorAll('[data-i18n-meta-content]').forEach(function (el) {
      el.setAttribute('content', t(el.getAttribute('data-i18n-meta-content')));
    });
    setDocumentTitle();
    renderToggle();
  }

  function setLang(lang) {
    LANG = lang === 'zh' ? 'zh' : 'en';
    try { localStorage.setItem(STORAGE_KEY, LANG); } catch (e) { /* ignore */ }
    I18N.lang = LANG;
    I18N.isZh = LANG === 'zh';
    applyTranslations();
    document.dispatchEvent(new CustomEvent('i18n:changed', { detail: { lang: LANG } }));
  }

  function toggle() {
    setLang(LANG === 'zh' ? 'en' : 'zh');
  }

  // ─── 导出 + 启动 ─────────────────────────────────────────────────────
  var I18N = {
    lang: LANG,
    isZh: false,
    copy: COPY,
    errors: ERRORS,
    pageTitle: PAGE_TITLE,
    t: t,
    tpl: tpl,
    errorText: errorText,
    localizeServerError: localizeServerError,
    detectLang: detectLang,
    setLang: setLang,
    toggle: toggle,
    applyTranslations: applyTranslations,
  };
  window.I18N = I18N;

  function init() {
    var detected = detectLang();
    LANG = detected.lang;
    if (detected.persist) {
      try { localStorage.setItem(STORAGE_KEY, LANG); } catch (e) { /* ignore */ }
    }
    I18N.lang = LANG;
    I18N.isZh = LANG === 'zh';
    applyTranslations();
  }

  // defer 脚本：文档已解析完毕可立即执行；若被内联引用则等 DOM 就绪。
  if (document.readyState === 'loading') {
    document.addEventListener('DOMContentLoaded', init);
  } else {
    init();
  }
})();
