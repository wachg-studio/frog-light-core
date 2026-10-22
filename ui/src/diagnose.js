// 游戏异常退出后，从最后几十行输出里认出常见原因，给一句人话和该怎么办。
// 返回 { k } 形式的提示（文案 key，显示时再翻译），认不出来返回 null。
const RULES = [
  [/OutOfMemoryError|Java heap space|GC overhead limit/i, '游戏内存不足。到 存档设置 → 游戏设置 里把内存调大一些，或者关掉几个模组再试。'],
  [/Could not create the Java Virtual Machine|Invalid maximum heap size|Could not reserve enough space/i, 'Java 虚拟机没能启动：内存设置可能超过了电脑的可用内存。把内存改回“自动”再试。'],
  [/UnsupportedClassVersionError|compiled by a more recent version of the Java Runtime|class file version \d+/i, '这个游戏（或模组）需要更新版本的 Java。如果你在设置里手动指定了 Java 路径，请清空它，让 FLC 自动选择。'],
  [/Mixin apply failed|MixinApplyError|InvalidMixinException|MixinTransformerError|Mixin transformation .* failed/i, '某个模组的修改和游戏或别的模组冲突了（Mixin 失败）。先停用最近新装的模组试试，并确认模组版本适配这个存档的游戏版本。'],
  [/requires (any )?version|Incompatible mods found|Missing or unsupported mandatory dependencies|Mod resolution failed|ModResolutionException|Could not find required mod|depends on|missing mandatory dependenc/i, '有模组缺少前置，或版本不兼容。打开日志找到 “requires / depends on” 那几行，在“模组与资源”里补装对应的前置。'],
  [/NoClassDefFoundError|ClassNotFoundException|NoSuchMethodError|NoSuchFieldError/i, '模组和游戏或加载器的版本对不上（缺少类或方法）。检查模组是否适配这个存档的游戏版本和加载器。'],
  [/GLFW error|WGL: The driver does not appear to support OpenGL|Pixel format not accelerated|OpenGL.*(not supported|failed)|EXCEPTION_ACCESS_VIOLATION.*(nvoglv|atio|ig\d)/i, '显卡驱动有问题：游戏没能创建图形窗口。请更新显卡驱动；如果是双显卡笔记本，在 Windows 的图形设置里给 Java 指定高性能显卡。'],
];

export function diagnose(lines) {
  const text = (lines ?? []).join('\n');
  for (const [re, k] of RULES) if (re.test(text)) return { k };
  return null;
}
