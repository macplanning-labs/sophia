"use client";

import { useQuery, useMutation, useQueryClient } from "@tanstack/react-query";
import { fetchPayrollDetail, recalculatePayrollDeductions } from "@/lib/api";
import { DetailLayout } from "@/components/detail-layout";
import { Badge } from "@/components/ui/badge";
import { Card, CardContent } from "@/components/ui/card";
import { getStatus } from "@/lib/status";
import { cn } from "@/lib/utils";
import { toast } from "sonner";
import { useDynamicId } from "@/lib/utils";
import { useCurrentUser } from "@/lib/useCurrentUser";

function formatYearMonth(ym?: string) {
  if (!ym) return "—";
  const [y, m] = ym.split("-");
  return m ? `${y}年${m}月` : ym;
}

function IdField({ label, value }: { label: string; value: React.ReactNode }) {
  return (
    <div className="px-4 py-2 sm:py-0 sm:first:pl-0 min-w-0">
      <p className="text-[11px] text-muted-foreground mb-0.5 truncate">{label}</p>
      <p className="text-sm font-semibold truncate">{value}</p>
    </div>
  );
}

function AttField({ label, value, sub, accent }: { label: string; value: React.ReactNode; sub?: React.ReactNode; accent?: boolean }) {
  return (
    <div className={cn("px-4 py-2 lg:py-0 lg:first:pl-0 text-center min-w-0")}>
      <p className={cn("text-[11px] mb-1", accent ? "text-primary" : "text-muted-foreground")}>{label}</p>
      <p className={cn("text-base font-bold tabular-nums", accent ? "text-primary" : "text-foreground")}>{value}</p>
      {sub && <p className="text-[10px] text-muted-foreground mt-0.5">{sub}</p>}
    </div>
  );
}

function LedgerItem({ label, value, muted, negative }: { label: string; value: string; muted?: boolean; negative?: boolean }) {
  return (
    <div className={cn("flex items-baseline justify-between gap-2 py-1.5 border-b border-border/60 text-sm", muted && "text-muted-foreground")}>
      <span className="text-muted-foreground">{label}</span>
      <span className={cn("font-semibold tabular-nums", negative && "text-red-400")}>{value}</span>
    </div>
  );
}

interface Props {
  /** When opened from list modal; falls back to useDynamicId() */
  id?: string;
  embedded?: boolean;
}

export default function PayrollDetailPage({ id: idProp, embedded = false }: Props = {}) {
  const dynamicId = useDynamicId();
  const id = idProp || dynamicId;
  const queryClient = useQueryClient();
  const { canViewAllPayroll } = useCurrentUser();
  const { data, isLoading, isError, error } = useQuery({
    queryKey: ["payroll", id],
    queryFn: () => fetchPayrollDetail(id),
    enabled: !!id,
  });

  /** 詳細・一覧の両方を更新（一覧だけ残ると「保存されていない」ように見える） */
  const invalidatePayroll = () => {
    queryClient.invalidateQueries({ queryKey: ["payroll"] });
  };

  const markPaid = useMutation({
    mutationFn: () => fetch(`/api/v1/payroll/${id}/paid`, { method: "POST" }).then(async r => { if (!r.ok) { const b = await r.json().catch(() => ({})); throw new Error(b.error || "支払済み更新に失敗しました"); } return r; }),
    onSuccess: () => { invalidatePayroll(); toast.success("支払済みに更新しました"); },
    onError: (e: Error) => toast.error(`支払済み更新に失敗しました: ${e.message}`),
  });

  const recalcDeductions = useMutation({
    mutationFn: () => recalculatePayrollDeductions(id),
    onSuccess: (res) => {
      if (res.success) {
        invalidatePayroll();
        toast.success(res.message ?? "控除を再計算して保存しました");
      } else {
        toast.error(res.error ?? "控除の再計算に失敗しました");
      }
    },
    onError: (e: Error) => toast.error(`控除の再計算に失敗しました: ${e.message}`),
  });

  const pay = data?.payroll;
  const st = pay ? getStatus("payroll", pay.status) : null;
  const nearestExpiry = data?.paid_leave_nearest_expiry as { days: string; expire_date: string } | undefined;

  const actions = canViewAllPayroll || st ? (
    <div className="flex items-center gap-2 flex-wrap">
      {st && <Badge variant="outline" className={cn("text-[11px]", st.className)}>{st.label}</Badge>}
      {canViewAllPayroll && (
        <button
          type="button"
          onClick={() => {
            if (confirm("支給額はそのままに、社会保険・所得税・住民税を社員マスタと料率表から再計算し、明細に保存します。よろしいですか？")) {
              recalcDeductions.mutate();
            }
          }}
          disabled={recalcDeductions.isPending}
          className="px-3 py-1.5 text-xs font-medium rounded-md bg-sky-500/10 text-sky-400 border border-sky-500/30 hover:bg-sky-500/20 transition-colors disabled:opacity-50"
        >
          {recalcDeductions.isPending ? "計算・保存中…" : "控除を再計算して保存"}
        </button>
      )}
      {canViewAllPayroll && pay?.status !== "PAID" && (
        <button
          type="button"
          onClick={() => { if (confirm("支払済みにしますか？")) markPaid.mutate(); }}
          disabled={markPaid.isPending}
          className="px-3 py-1.5 text-xs font-medium rounded-md bg-emerald-500/10 text-emerald-400 border border-emerald-500/30 hover:bg-emerald-500/20 transition-colors disabled:opacity-50"
        >
          ✅ 支払済みにする
        </button>
      )}
    </div>
  ) : undefined;

  const layoutActions = pay && actions
    ? (embedded
      ? <div className="sticky top-0 z-10 -mx-1 px-1 py-2 mb-1 bg-card/95 backdrop-blur-sm border-b border-border">{actions}</div>
      : actions)
    : undefined;

  return (
    <DetailLayout title={`給与明細 #${id}`} icon="💰" backHref="/payroll" backLabel="一覧に戻る" isLoading={isLoading} actions={layoutActions} embedded={embedded}>
      {isError && (
        <Card className="border-red-500/30 bg-red-500/5">
          <CardContent className="p-4 text-sm text-red-400">
            給与明細の取得に失敗しました{error instanceof Error && error.message ? `：${error.message}` : ""}
          </CardContent>
        </Card>
      )}

      {pay && (
        <div className="space-y-4">
          {/* 基本情報 */}
          <Card className="bg-card border-border">
            <CardContent className="grid grid-cols-2 sm:grid-cols-4 divide-y divide-border sm:divide-y-0 sm:divide-x">
              <IdField label="会社名" value={data.company_name || "—"} />
              <IdField label="氏名" value={<>{data.employee_name}<span className="text-muted-foreground font-normal ml-1.5">{data.employee_code}</span></>} />
              <IdField label="対象月" value={formatYearMonth(pay.year_month)} />
              <IdField label="支給日" value={pay.payment_date ?? "未定"} />
            </CardContent>
          </Card>

          {/* 支給・控除・差引支給額 */}
          <div className="grid grid-cols-1 sm:grid-cols-3 gap-4">
            <Card className="bg-card border-border">
              <CardContent>
                <p className="text-xs font-semibold text-muted-foreground mb-1">支給合計</p>
                <p className="text-2xl font-bold text-emerald-400 tabular-nums">¥{Number(pay.gross_pay).toLocaleString()}</p>
              </CardContent>
            </Card>
            <Card className="bg-card border-border">
              <CardContent>
                <p className="text-xs font-semibold text-muted-foreground mb-1">控除合計</p>
                <p className="text-2xl font-bold text-red-400 tabular-nums">¥{Number(pay.deduction_total).toLocaleString()}</p>
              </CardContent>
            </Card>
            <Card className="bg-amber-500/10 border-amber-500/30 ring-amber-500/20">
              <CardContent>
                <p className="text-xs font-semibold text-amber-400/80 mb-1">差引支給額（手取り）</p>
                <p className="text-3xl font-extrabold text-amber-400 tabular-nums">¥{Number(pay.net_pay).toLocaleString()}</p>
              </CardContent>
            </Card>
          </div>

          {/* 勤怠・有給情報 */}
          <Card className="bg-card border-border">
            <CardContent className="grid grid-cols-2 sm:grid-cols-4 lg:grid-cols-7 divide-y divide-border sm:divide-y-0 lg:divide-x">
              <AttField label="出勤日数" value={`${pay.work_days ?? 0}日`} />
              <AttField label="欠勤日数" value={`${pay.absence_days ?? 0}日`} />
              <AttField label="残業時間" value={`${Number(pay.overtime_hours ?? 0)}h`} />
              <AttField label="深夜時間" value={`${Number(pay.night_hours ?? 0)}h`} />
              <AttField label="休日時間" value={`${Number(pay.holiday_hours ?? 0)}h`} />
              <AttField label="今月有給" value={`${Number(pay.paid_leave_used_days ?? 0)}日`} accent />
              <AttField
                label="有給残"
                value={`${Number(pay.paid_leave_balance_days ?? 0)}日`}
                accent
                sub={nearestExpiry ? `うち${Number(nearestExpiry.days)}日は${nearestExpiry.expire_date}失効` : undefined}
              />
            </CardContent>
          </Card>

          {/* 支給・控除 内訳 */}
          <div className="grid grid-cols-1 lg:grid-cols-2 gap-4">
            <Card className="bg-card border-border">
              <CardContent>
                <p className="text-xs font-bold mb-2 flex items-center gap-2"><span className="w-1.5 h-1.5 rounded-full bg-emerald-400" />支給項目</p>
                <LedgerItem label="基本給" value={`¥${Number(pay.base_salary ?? 0).toLocaleString()}`} />
                <LedgerItem label="役職手当" value={`¥${Number(pay.position_allowance ?? 0).toLocaleString()}`} />
                <LedgerItem label="住宅手当" value={`¥${Number(pay.housing_allowance ?? 0).toLocaleString()}`} />
                <LedgerItem label="通勤手当" value={`¥${Number(pay.commuting_allowance ?? 0).toLocaleString()}`} />
                <LedgerItem label="時間外手当" value={`¥${Number(pay.overtime_pay ?? 0).toLocaleString()}`} />
                <LedgerItem label="深夜手当" value={`¥${Number(pay.night_pay ?? 0).toLocaleString()}`} />
                <LedgerItem label="休日手当" value={`¥${Number(pay.holiday_pay ?? 0).toLocaleString()}`} />
                <LedgerItem label="欠勤控除" value={`▲¥${Number(pay.absence_deduction ?? 0).toLocaleString()}`} negative />
                <div className="flex items-center justify-between pt-3 mt-1">
                  <span className="text-sm font-bold">支給合計</span>
                  <span className="text-lg font-bold text-emerald-400 tabular-nums">¥{Number(pay.gross_pay ?? 0).toLocaleString()}</span>
                </div>
              </CardContent>
            </Card>

            <Card className="bg-card border-border">
              <CardContent>
                <p className="text-xs font-bold mb-2 flex items-center gap-2"><span className="w-1.5 h-1.5 rounded-full bg-red-400" />控除項目</p>
                <LedgerItem label="健康保険" value={`¥${Number(pay.health_premium ?? 0).toLocaleString()}`} />
                <LedgerItem label="介護保険" value={`¥${Number(pay.nursing_premium ?? 0).toLocaleString()}`} />
                <LedgerItem label="厚生年金" value={`¥${Number(pay.pension_premium ?? 0).toLocaleString()}`} />
                <LedgerItem label="雇用保険" value={`¥${Number(pay.employment_premium ?? 0).toLocaleString()}`} />
                <LedgerItem label="（社会保険料計）" value={`¥${Number(pay.social_insurance_total ?? 0).toLocaleString()}`} muted />
                <LedgerItem label="所得税" value={`¥${Number(pay.income_tax ?? 0).toLocaleString()}`} />
                <LedgerItem label="住民税" value={`¥${Number(pay.resident_tax ?? 0).toLocaleString()}`} />
                <div className="flex items-center justify-between pt-3 mt-1">
                  <span className="text-sm font-bold">控除合計</span>
                  <span className="text-lg font-bold text-red-400 tabular-nums">¥{Number(pay.deduction_total ?? 0).toLocaleString()}</span>
                </div>
              </CardContent>
            </Card>
          </div>
        </div>
      )}
    </DetailLayout>
  );
}
