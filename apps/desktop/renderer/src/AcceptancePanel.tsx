import { useEffect, useRef, useState, type ReactNode } from "react";
import {
  AlertCircle,
  Check,
  Download,
  Loader2,
  Play,
  RotateCcw,
  ShieldCheck,
  Square,
  Trash2
} from "lucide-react";
import {
  desktopApi,
  type AcceptanceExportReceipt,
  type AcceptanceLocationEvidence,
  type AcceptanceReport,
  type AcceptanceRuntimeStatus,
  type AcceptanceScenario,
  type AcceptanceScenarioLatencySummary,
  type AcceptanceScenarioOrdinal,
  type AcceptanceSelectionRecord,
  type AcceptanceWatcherResourceCounts,
  type SettingsPayload
} from "./desktopApi";

type ScenarioOption = {
  value: AcceptanceScenario;
  label: string;
  maximum: number;
  instruction: string;
  automatedRestart?: boolean;
};

const SCENARIOS: ScenarioOption[] = [
  { value: "textEditDrag", label: "TextEdit 普通拖选", maximum: 30, instruction: "连续拖选固定 Drag target。" },
  { value: "textEditDoubleClick", label: "TextEdit 双击选词", maximum: 30, instruction: "连续双击固定 orbit。" },
  { value: "textEditTripleClick", label: "TextEdit 三击选段", maximum: 30, instruction: "连续三击固定段落。" },
  {
    value: "textEditShiftExtend",
    label: "TextEdit Shift 扩选",
    maximum: 30,
    instruction: "在 amber 前放置插入点，按住 Shift 点击 ember 末尾；mouse-up 时仍须按住 Shift。"
  },
  {
    value: "textEditKeyboardSelection",
    label: "TextEdit 键盘选区",
    maximum: 30,
    instruction: "在 Silent 前放置插入点，再按 Shift+Command+→ 选到该行末尾。"
  },
  {
    value: "sameTextSameLocation",
    label: "同位置重复文本",
    maximum: 30,
    instruction: "每次都用普通鼠标拖选位置 A 的 repeatable phrase；不得改用 AX-only 键盘选区。"
  },
  {
    value: "sameTextDifferentLocation",
    label: "异位置重复文本",
    maximum: 30,
    instruction: "用普通鼠标拖选，奇数次选位置 A、偶数次选位置 B；后端会从真实 mouse-up 位置判定 A/B。"
  },
  {
    value: "safariFixture",
    label: "Safari 固定 HTML",
    maximum: 1,
    instruction: "只执行一次 dedicated sample：拖选固定 HTML 中的 The same selection appears here."
  },
  {
    value: "previewFixture",
    label: "Preview 固定 PDF",
    maximum: 1,
    instruction: "只执行一次 dedicated sample：拖选固定 PDF 中的 The same selection appears here."
  },
  {
    value: "rapidAThenB",
    label: "快速 A→B 竞态",
    maximum: 20,
    instruction:
      "每组先真实选择固定 A；完成 A 手势后不要等待浮窗，立即在 300 ms 内真实选择固定 B。A 提交后工具会自动启动无网络的旧写入探针；约 600 ms 后该探针必须被控制器拒绝。"
  },
  { value: "tapDisabledRecovery", label: "事件监听恢复", maximum: 1, instruction: "在已批准的验收构建中触发 tap-disabled；2 秒内必须恢复或明确降级。" },
  {
    value: "watcherRestart",
    label: "监听重启 50 次",
    maximum: 50,
    instruction: "工具将依次重启并采集每轮资源快照。",
    automatedRestart: true
  }
];

const SELECTION_SCENARIOS: AcceptanceScenario[] = [
  "textEditDrag",
  "textEditDoubleClick",
  "textEditTripleClick",
  "textEditShiftExtend",
  "textEditKeyboardSelection",
  "sameTextSameLocation",
  "sameTextDifferentLocation",
  "safariFixture",
  "previewFixture"
];

const EVIDENCE_MISSING = "证据缺失";

interface AcceptancePanelProps {
  onRuntimeStatusChange?: (payload: SettingsPayload) => void;
}

export function AcceptancePanel({ onRuntimeStatusChange }: AcceptancePanelProps): JSX.Element | null {
  const [status, setStatus] = useState<AcceptanceRuntimeStatus | null>(null);
  const [statusLoadError, setStatusLoadError] = useState<string | null>(null);
  const [statusLoadAttempt, setStatusLoadAttempt] = useState(0);
  const [report, setReport] = useState<AcceptanceReport | null>(null);
  const [receipt, setReceipt] = useState<AcceptanceExportReceipt | null>(null);
  const [scenario, setScenario] = useState<AcceptanceScenario>(SCENARIOS[0].value);
  const [startingOrdinal, setStartingOrdinal] = useState("1");
  const [busy, setBusy] = useState(false);
  const [sequenceProgress, setSequenceProgress] = useState<string | null>(null);
  const [message, setMessage] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [clearArmed, setClearArmed] = useState(false);
  const stopSequenceRef = useRef(false);

  useEffect(() => {
    let alive = true;
    void desktopApi
      .getAcceptanceStatus()
      .then((next) => {
        if (alive) {
          setStatusLoadError(null);
          setStatus(next.enabled ? next : null);
        }
      })
      .catch((loadError: unknown) => {
        if (alive) {
          setStatus(null);
          setStatusLoadError(
            formatAcceptanceError(loadError, "无法读取验收工具状态。")
          );
        }
      });
    return () => {
      alive = false;
      stopSequenceRef.current = true;
    };
  }, [statusLoadAttempt]);

  useEffect(() => {
    if (!status?.enabled || status.phase !== "running" || busy) {
      return;
    }
    const timer = window.setInterval(() => {
      void desktopApi.getAcceptanceStatus().then(setStatus).catch(() => undefined);
    }, 1_000);
    return () => window.clearInterval(timer);
  }, [busy, status?.enabled, status?.phase]);

  if (!status) {
    if (statusLoadError) {
      return (
        <section className="acceptance-panel" aria-labelledby="acceptance-load-error-title">
          <div className="acceptance-heading">
            <div>
              <p className="eyebrow">Reliability gate</p>
              <h2 id="acceptance-load-error-title">验收工具不可用</h2>
            </div>
          </div>
          <div className="acceptance-feedback error" role="alert">
            <AlertCircle size={15} />
            <span>{statusLoadError}</span>
          </div>
          <button
            type="button"
            className="inline-action"
            onClick={() => setStatusLoadAttempt((attempt) => attempt + 1)}
          >
            <RotateCcw size={14} />
            重试读取验收状态
          </button>
        </section>
      );
    }
    return null;
  }

  const selected = SCENARIOS.find((entry) => entry.value === scenario) ?? SCENARIOS[0];
  const ordinal = Number(startingOrdinal);
  const ordinalValid = Number.isInteger(ordinal) && ordinal >= 1 && ordinal <= selected.maximum;

  async function startSession(): Promise<void> {
    await runAction(async () => {
      setReport(null);
      setReceipt(null);
      setStatus(await desktopApi.startAcceptanceDiagnostics());
      setMessage("固定 full_baseline 验收会话已开始。请先核对 fixture 与场景说明。");
    });
  }

  async function armOne(): Promise<void> {
    if (!ordinalValid) {
      setError(`序号必须在 1–${selected.maximum} 之间。`);
      return;
    }
    await runAction(async () => {
      setStatus(await desktopApi.armAcceptanceScenario(scenario, ordinal));
      setMessage(`已准备 ${selected.label} · 第 ${ordinal}/${selected.maximum} 次。`);
    });
  }

  async function runSequence(): Promise<void> {
    if (!ordinalValid) {
      setError(`起始序号必须在 1–${selected.maximum} 之间。`);
      return;
    }
    stopSequenceRef.current = false;
    setBusy(true);
    setError(null);
    setMessage(null);
    try {
      for (let current = ordinal; current <= selected.maximum; current += 1) {
        if (stopSequenceRef.current) {
          setMessage(`连续计数已在第 ${current}/${selected.maximum} 次之前停止。`);
          break;
        }
        const armedStatus = await desktopApi.armAcceptanceScenario(scenario, current);
        setStatus(armedStatus);
        setSequenceProgress(`${selected.label} · 已准备 ${current}/${selected.maximum}`);
        if (selected.automatedRestart) {
          const refreshed = await desktopApi.refreshWatcherStatus();
          onRuntimeStatusChange?.(refreshed);
        } else if (scenario === "tapDisabledRecovery" && armedStatus.tapInjectionAvailable) {
          await desktopApi.injectAcceptanceTapDisabled();
          await waitForTapResolution(setStatus);
          onRuntimeStatusChange?.(await desktopApi.getSettings());
          continue;
        }
        await waitUntilArmCompletes(scenario, current, stopSequenceRef, setStatus);
      }
      if (!stopSequenceRef.current) {
        setMessage(`${selected.label} 的固定次数已全部记录；最终是否通过以结束后的报告为准。`);
      }
    } catch (sequenceError) {
      setError(formatAcceptanceError(sequenceError, "连续验收中断。"));
    } finally {
      setSequenceProgress(null);
      setBusy(false);
      void desktopApi.getAcceptanceStatus().then(setStatus).catch(() => undefined);
    }
  }

  async function toggleSelfIsolation(): Promise<void> {
    const action = status?.selfIsolationActive ? "end" : "start";
    await runAction(async () => {
      setStatus(await desktopApi.setAcceptanceSelfIsolation(action));
      setMessage(
        action === "start"
          ? "自身窗口隔离计时已开始；请连续操作设置页与浮窗至少 5 分钟。"
          : "自身窗口隔离计时已结束；结果将在报告中按时长和触发次数判定。"
      );
    });
  }

  async function injectTapDisabled(): Promise<void> {
    await runAction(async () => {
      await desktopApi.injectAcceptanceTapDisabled();
      const resolved = await waitForTapResolution(setStatus);
      setStatus(resolved);
      onRuntimeStatusChange?.(await desktopApi.getSettings());
      setMessage("tap-disabled 已注入；恢复或明确降级时延将由后端固定阈值判定。 ");
    });
  }

  async function endSession(): Promise<void> {
    await runAction(async () => {
      const nextReport = await desktopApi.endAcceptanceDiagnostics();
      setReport(nextReport);
      setStatus(await desktopApi.getAcceptanceStatus());
      setMessage(nextReport.status.valid ? "验收报告通过。" : "验收报告未通过；请查看缺失项后重新执行。 ");
    });
  }

  async function exportReport(): Promise<void> {
    await runAction(async () => {
      const nextReceipt = await desktopApi.exportAcceptanceDiagnostics();
      setReceipt(nextReceipt);
      setMessage("不含选中文本的 JSON 证据已原子导出。 ");
    });
  }

  async function clearSession(): Promise<void> {
    if (!clearArmed) {
      setClearArmed(true);
      setMessage("再次点击“确认清除”才会删除当前内存会话和已导出的验收报告。 ");
      return;
    }
    await runAction(async () => {
      stopSequenceRef.current = true;
      setStatus(await desktopApi.clearAcceptanceDiagnostics());
      setReport(null);
      setReceipt(null);
      setClearArmed(false);
      setMessage("验收会话和导出报告已清除。 ");
    });
  }

  async function runAction(action: () => Promise<void>): Promise<void> {
    setBusy(true);
    setError(null);
    setMessage(null);
    try {
      await action();
    } catch (actionError) {
      setError(formatAcceptanceError(actionError, "验收操作失败。"));
    } finally {
      setBusy(false);
    }
  }

  return (
    <section className="acceptance-panel" aria-labelledby="acceptance-title">
      <div className="panel-heading">
        <div className="section-icon">
          <ShieldCheck size={18} />
        </div>
        <div>
          <p className="eyebrow">仅限显式验收模式</p>
          <h2 id="acceptance-title">划词可靠性验收</h2>
          <p>固定阈值由后端提供；报告只记录计数、修订号、来源标识与单调时延，不保存任何文本内容。</p>
        </div>
      </div>

      <div className="acceptance-status-grid" aria-label="验收会话状态">
        <div><span>阶段</span><strong>{formatPhase(status.phase)}</strong></div>
        <div><span>固定方案</span><strong>{status.preset}</strong></div>
        <div><span>已记录 generation</span><strong>{status.recordedGenerations}</strong></div>
        <div><span>重复回调抑制</span><strong>{status.duplicateGenerationsSuppressed}</strong></div>
        <div><span>待完成 A→B guard</span><strong>{status.pendingRapidProbes}</strong></div>
      </div>

      {status.phase === "running" ? (
        <div className="acceptance-controls">
          <label className="field">
            <span>验收场景</span>
            <select value={scenario} onChange={(event) => setScenario(event.target.value as AcceptanceScenario)} disabled={busy}>
              {SCENARIOS.map((entry) => <option key={entry.value} value={entry.value}>{entry.label}</option>)}
            </select>
            <small>{selected.instruction} 固定次数：{selected.maximum}。</small>
          </label>
          <label className="field acceptance-ordinal">
            <span>起始序号</span>
            <input
              type="number"
              min={1}
              max={selected.maximum}
              value={startingOrdinal}
              onChange={(event) => setStartingOrdinal(event.target.value)}
              disabled={busy}
            />
          </label>
          <div className="acceptance-button-row">
            <button type="button" className="secondary" onClick={() => void armOne()} disabled={busy || !ordinalValid}>
              <Play size={15} />准备单次
            </button>
            <button type="button" className="secondary" onClick={() => void runSequence()} disabled={busy || !ordinalValid}>
              {busy ? <Loader2 size={15} className="spin" /> : <RotateCcw size={15} />}
              {selected.automatedRestart ? "自动执行剩余重启" : "连续记录至固定次数"}
            </button>
            {busy ? (
              <button type="button" className="secondary danger" onClick={() => { stopSequenceRef.current = true; }}>
                <Square size={14} />停止连续计数
              </button>
            ) : null}
          </div>
          {sequenceProgress ? <p className="acceptance-progress" role="status">{sequenceProgress}</p> : null}
          {status.armed ? (
            <p className="acceptance-armed" role="status">
              当前已准备：{scenarioLabel(status.armed.scenario)} · 第 {status.armed.ordinal} 次。现在执行对应真实操作。
            </p>
          ) : null}
          {scenario === "tapDisabledRecovery" ? (
            <div className="acceptance-button-row">
              {status.tapInjectionAvailable ? (
                <button
                  type="button"
                  className="secondary"
                  onClick={() => void injectTapDisabled()}
                  disabled={busy || status.armed?.scenario !== "tapDisabledRecovery"}
                >
                  <RotateCcw size={15} />注入一次 tap-disabled
                </button>
              ) : (
                <small>当前构建没有测试注入能力；请只在获批准的系统环境中触发真实 tap-disabled。</small>
              )}
            </div>
          ) : null}
          <div className="acceptance-button-row">
            <button type="button" className="secondary" onClick={() => void toggleSelfIsolation()} disabled={busy}>
              {status.selfIsolationActive ? <Square size={14} /> : <Play size={15} />}
              {status.selfIsolationActive ? "结束自身窗口隔离" : "开始 5 分钟自身窗口隔离"}
            </button>
            <button type="button" className="primary" onClick={() => void endSession()} disabled={busy || status.selfIsolationActive}>
              <Check size={15} />结束并汇总
            </button>
          </div>
        </div>
      ) : null}

      {status.phase === "idle" ? (
        <button type="button" className="primary" onClick={() => void startSession()} disabled={busy}>
          {busy ? <Loader2 size={15} className="spin" /> : <Play size={15} />}
          开始固定验收会话
        </button>
      ) : null}

      {report ? <AcceptanceReportSummary report={report} /> : null}

      {status.phase === "ended" ? (
        <div className="acceptance-button-row">
          <button type="button" className="primary" onClick={() => void exportReport()} disabled={busy}>
            <Download size={15} />导出无内容 JSON
          </button>
          <button type="button" className="secondary danger" onClick={() => void clearSession()} disabled={busy}>
            <Trash2 size={15} />{clearArmed ? "确认清除" : "清除会话"}
          </button>
        </div>
      ) : null}

      {receipt ? <p className="acceptance-receipt">证据路径：<code>{receipt.path}</code></p> : null}
      <div className={error ? "acceptance-feedback error" : "acceptance-feedback"} role={error ? "alert" : "status"} aria-live="polite">
        {error ? <><AlertCircle size={15} /><span>{error}</span></> : null}
        {!error && message ? <><Check size={15} /><span>{message}</span></> : null}
      </div>
    </section>
  );
}

function AcceptanceReportSummary({ report }: { report: AcceptanceReport }): JSX.Element {
  return (
    <div className="acceptance-report" data-valid={report.status.valid}>
      <strong>{report.status.valid ? "固定验收通过" : "固定验收未通过"}</strong>
      <span>记录 {report.recordedRecords} 条 · 选区 revision {report.summary.uniqueSelectionRevisions} 个 · popup revision {report.summary.uniquePopupRevisions} 个</span>
      <span>多击安静窗口 Q：{report.multiClickQuietWindowMs} ms · 证据版本 {report.reportVersion}</span>
      <span>A→B：{report.summary.rapidAToB.finalBPairs}/{report.summary.rapidAToB.expectedPairs} · watcher 样本：{report.summary.watcherResources.sampleCount}</span>
      <span>
        tap 恢复：{report.summary.tapRecovery.passed ? "通过" : "未通过"} · 最大时延：
        {report.summary.tapRecovery.maxResolutionMs === null
          ? "无终态样本"
          : `${report.summary.tapRecovery.maxResolutionMs} ms`}
      </span>
      <span>
        watcher 资源：{report.summary.watcherResources.passed ? "通过" : "未通过"} · 样本：
        {report.summary.watcherResources.sampleCount}
      </span>
      {!report.status.valid ? <small>失败原因：{report.status.invalidReasons.join("、") || "未满足固定阈值"}</small> : null}
    </div>
  );
}

async function waitUntilArmCompletes(
  scenario: AcceptanceScenario,
  ordinal: number,
  stop: { current: boolean },
  update: (status: AcceptanceRuntimeStatus) => void
): Promise<void> {
  while (!stop.current) {
    await new Promise((resolve) => window.setTimeout(resolve, 150));
    const next = await desktopApi.getAcceptanceStatus();
    update(next);
    if (!next.armed) {
      return;
    }
    if (next.armed.scenario !== scenario || next.armed.ordinal !== ordinal) {
      throw new Error("验收场景在连续计数期间被其他操作替换。 ");
    }
  }
}

async function waitForTapResolution(
  update: (status: AcceptanceRuntimeStatus) => void
): Promise<AcceptanceRuntimeStatus> {
  const deadline = performance.now() + 2_500;
  while (performance.now() < deadline) {
    await new Promise((resolve) => window.setTimeout(resolve, 50));
    const next = await desktopApi.getAcceptanceStatus();
    update(next);
    if (!next.armed) {
      return next;
    }
  }
  throw new Error("tap-disabled 注入后 2.5 秒内没有得到终态。 ");
}

function formatPhase(phase: AcceptanceRuntimeStatus["phase"]): string {
  return phase === "idle" ? "未开始" : phase === "running" ? "记录中" : "已汇总";
}

function scenarioLabel(scenario: AcceptanceScenario): string {
  return SCENARIOS.find((entry) => entry.value === scenario)?.label ?? scenario;
}

function formatAcceptanceError(error: unknown, fallback: string): string {
  return error instanceof Error && error.message.trim() ? error.message : fallback;
}
