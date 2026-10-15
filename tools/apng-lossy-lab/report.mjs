// 本地报告仅引用隔离目录中的文件；外部浏览器观感和数值验证分开。
const escape = (value) =>
  String(value)
    .replaceAll('&', '&amp;')
    .replaceAll('<', '&lt;')
    .replaceAll('>', '&gt;')
    .replaceAll('"', '&quot;');
export function htmlReport(report) {
  const sections = report.samples
    .filter((s) => s.group !== 'structural' && s.baseline)
    .map((sample, index) => {
      const rows = report.rows.filter((r) => r.sample === sample.file && r.result === 'candidate');
      const first =
        rows.find(
          (r) =>
            r.options.quality === 80 &&
            !r.options.dithering &&
            r.options.remapping === 'alpha_palette',
        ) ?? rows[0];
      const table = rows
        .map(
          (r, i) =>
            '<tr><td>' +
            r.options.quality +
            '</td><td>' +
            escape(r.options.remapping) +
            '</td><td>' +
            r.report.comparison.minimumPixelQuality +
            '</td><td>' +
            (r.options.dithering ? '开' : '关') +
            '</td><td>' +
            r.candidateBytes +
            '</td><td>' +
            r.extraSavedBytes +
            '</td><td>' +
            r.report.paletteEntries +
            '</td><td>' +
            Math.max(
              r.report.comparison.black.worstFrameRmse,
              r.report.comparison.white.worstFrameRmse,
            ).toFixed(3) +
            '</td><td>' +
            Math.max(
              r.report.comparison.black.worstTemporalRmse,
              r.report.comparison.white.worstTemporalRmse,
            ).toFixed(3) +
            '</td><td>' +
            escape(r.screening) +
            '<br><small>' +
            escape((r.report.qualityRejections ?? []).join(', ')) +
            '</small>' +
            '</td><td><button data-group="' +
            index +
            '" data-path="' +
            escape(r.output) +
            '">查看 ' +
            (i + 1) +
            '</button></td></tr>',
        )
        .join('');
      const figure = (file, label, id = '') =>
        '<figure><img ' +
        id +
        ' loading="lazy" src="' +
        escape(file) +
        '" alt="' +
        escape(label) +
        '"><figcaption>' +
        escape(label) +
        '</figcaption></figure>';
      return (
        '<section><h2>' +
        escape(sample.file) +
        '</h2><p>原件 ' +
        sample.inputBytes +
        ' bytes；无损有效基线 ' +
        sample.baseline.effectiveBytes +
        ' bytes。</p><div class="views">' +
        figure(sample.file, '原件') +
        figure(sample.baseline.file, '严格无损 O1') +
        (first
          ? figure(
              first.output,
              'q' + first.options.quality + ' · ' + first.screening,
              'id="candidate-' + index + '"',
            )
          : '') +
        '</div><div class="scroll"><table><thead><tr><th>质量</th><th>策略</th><th>实际像素评分</th><th>抖动</th><th>bytes</th><th>较基线节省</th><th>色表项</th><th>最差帧RMSE</th><th>最差时域RMSE</th><th>筛选</th><th>候选</th></tr></thead><tbody>' +
        table +
        '</tbody></table></div></section>'
      );
    })
    .join('');
  return (
    '<!doctype html><html lang="zh-CN"><meta charset="utf-8"><meta name="viewport" content="width=device-width"><title>APNG共享调色板实验</title><style>body{font:16px/1.6 system-ui,sans-serif;margin:28px auto;max-width:1380px;padding:0 24px;color:#e6edf3;background:#17202b}h1,h2{line-height:1.3}a{color:#9bd7ff}section{border-top:1px solid #536377;padding:24px 0}.views{display:flex;flex-wrap:wrap;gap:16px}.views img{display:block;max-width:360px;max-height:320px;object-fit:contain;background:repeating-conic-gradient(#eee 0% 25%,#bbb 0% 50%) 0/16px 16px}figure{margin:0;max-width:360px}figcaption{font-size:13px;overflow-wrap:anywhere}.scroll{overflow:auto}table{border-collapse:collapse;width:100%;margin-top:20px}th,td{text-align:left;padding:8px;border-bottom:1px solid #536377;font-variant-numeric:tabular-nums}button,select{font:inherit;padding:4px 8px}.black .views img{background:#000}.white .views img{background:#fff}</style><h1>APNG共享调色板有损实验</h1><p>状态：' +
    escape(report.result) +
    '。通过数值筛选不等于人工观感验收；所有质量与收益拒绝均保留。浏览器动画不保证同步，原始延时、循环与帧控制由独立比较器检查。</p><p><a href="report.json">完整报告（含结构/拒绝样本）</a> · <a href="engine-identity.json">构建身份</a> · <a href="SOURCES.json">公开原件来源和许可</a></p><label>透明背景 <select id="background"><option value="checker">棋盘</option><option value="black">黑色</option><option value="white">白色</option></select></label>' +
    sections +
    '<script>document.getElementById("background").addEventListener("change",e=>{document.body.className=e.target.value});document.addEventListener("click",e=>{const b=e.target.closest("button[data-path]");if(!b)return;const img=document.getElementById("candidate-"+b.dataset.group);img.src=b.dataset.path;img.nextElementSibling.textContent=b.dataset.path;});</script></html>'
  );
}
