import AppKit
let width=820, height=680
let bitmap=NSBitmapImageRep(bitmapDataPlanes:nil,pixelsWide:width,pixelsHigh:height,bitsPerSample:8,samplesPerPixel:4,hasAlpha:true,isPlanar:false,colorSpaceName:.deviceRGB,bytesPerRow:width*4,bitsPerPixel:32)!
NSGraphicsContext.saveGraphicsState()
NSGraphicsContext.current=NSGraphicsContext(bitmapImageRep:bitmap)
NSColor(calibratedRed:0.97,green:0.97,blue:0.95,alpha:1).setFill()
NSBezierPath(rect:NSRect(x:0,y:0,width:width,height:height)).fill()
func text(_ value:String,_ x:CGFloat,_ top:CGFloat,_ size:CGFloat,_ bold:Bool=false,_ color:NSColor = .black){
 let font=bold ? NSFont.boldSystemFont(ofSize:size) : NSFont.systemFont(ofSize:size)
 (value as NSString).draw(at:NSPoint(x:x,y:CGFloat(height)-top-size-5),withAttributes:[.font:font,.foregroundColor:color])
}
text("好好写剧本",44,26,29,true)
text("拖动安装 · Drag to install",44,73,18,false,.darkGray)
text("→",384,185,46,false,NSColor(calibratedRed:0.32,green:0.40,blue:0.51,alpha:1))
text("把左边的软件拖到右边的「应用程序」文件夹",44,290,19,true)
text("Drag the app into Applications, then open it from there.",44,325,16)
let panel=NSBezierPath(roundedRect:NSRect(x:30,y:22,width:760,height:300),xRadius:16,yRadius:16)
NSColor.white.setFill();panel.fill()
text("首次打开被拦截？按这几步来",48,374,20,true)
text("1. 安全弹窗点「完成」，不要移到废纸篓。",48,413,17)
text("2. 系统设置 → 隐私与安全性 → 安全性 →「仍要打开」。",48,443,17)
text("3. 找不到按钮？退出系统设置，再重新进入这个页面。",48,473,17)
text("First launch blocked?",48,520,18,true)
text("Click Done → System Settings → Privacy & Security → Open Anyway.",48,551,15)
text("No button? Close System Settings and reopen that page.",48,578,15)
text("测试版尚未经过 Apple 公证 · This beta is not Apple-notarized.",48,620,14,false,.darkGray)
NSGraphicsContext.restoreGraphicsState()
try bitmap.representation(using:.png,properties:[:])!.write(to:URL(fileURLWithPath:CommandLine.arguments[1]))
