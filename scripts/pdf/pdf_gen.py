#!/usr/bin/env python3
"""
PDF生成スクリプト — EDI版(reportlab)の流用
===========================================

Rustから呼び出され、stdoutにPDFバイナリを出力する。
Django依存を排除し、全データはJSON引数で受け取る。

対応帳票:
  - order:    注文書
  - acceptance: 注文請書
  - invoice:  請求書
  - notice:   支払通知書（検収兼お支払通知書）

使用方法:
  python3 pdf_gen.py --type order --json '{"order_id":"PO-202607-001",...}'
"""

import argparse
import json
import sys
import io
import os
import html

from reportlab.pdfgen import canvas
from reportlab.lib.pagesizes import A4
from reportlab.lib.units import mm
from reportlab.pdfbase import pdfmetrics
from reportlab.pdfbase.cidfonts import UnicodeCIDFont
from reportlab.platypus import Table, TableStyle, Paragraph
from reportlab.lib.styles import ParagraphStyle
from reportlab.lib import colors


# ============================================================
# 共通ユーティリティ（EDI: orders/services/pdf_generator.py 由来）
# ============================================================

STAMP_PATH = os.path.join(os.path.dirname(__file__), '..', '..', 'media', 'stamps', 'company_seal.png')
# Docker内パス
if not os.path.exists(STAMP_PATH):
    STAMP_PATH = '/app/media/stamps/company_seal.png'
# ローカルスクリプトディレクトリ内のフォールバック
if not os.path.exists(STAMP_PATH):
    STAMP_PATH = os.path.join(os.path.dirname(__file__), 'company_seal.png')


def _setup_fonts(p):
    """フォント登録（日本語対応 — HeiseiMin-W3: reportlab内蔵CIDフォント）"""
    font_name = "HeiseiMin-W3"
    try:
        pdfmetrics.getFont(font_name)
    except:
        pdfmetrics.registerFont(UnicodeCIDFont("HeiseiMin-W3"))
    return font_name


def _split_address(address, max_chars=25):
    """住所を適切な位置で分割する。

    分割ルール:
    1. 全角スペース or 半角スペースがあればそこで分割
    2. スペースがない場合、max_chars文字で強制分割
    3. 短い住所はそのまま1行

    例:
      "東京都港区虎ノ門1-17-1 虎ノ門ヒルズビジネスタワー5F"
      → ["東京都港区虎ノ門1-17-1", "虎ノ門ヒルズビジネスタワー5F"]
    """
    if not address or len(address) <= max_chars:
        return [address]

    # 全角スペース or 半角スペースで分割
    import re
    parts = re.split(r'[ 　]+', address, maxsplit=1)
    if len(parts) == 2:
        return parts

    # スペースなし → 強制分割
    return [address[:max_chars], address[max_chars:]]


def _draw_header_section(p, width, height, font_name,
                         addressee_label="（乙）", addressee_name="",
                         addressee_suffix="御中",
                         issuer_label="（甲）", issuer_info=None,
                         show_stamp=True):
    """宛先（左側）と発行者（右側）を自動バランスで配置し、次セクションのY座標を返す。

    EDI: orders/services/pdf_generator.py の _draw_header_section() を流用。
    """
    LINE_GAP = 8 * mm
    ADDR_NAME_FONT = 12
    ISSUER_NAME_FONT = 10
    RIGHT_MARGIN = 20 * mm
    STAMP_SIZE = 22 * mm
    STAMP_OVERLAP = 0.35

    if issuer_info is None:
        issuer_info = {'name': '', 'postal_code': '', 'address': '', 'tel': '', 'fax': ''}

    # --- 宛先セクション描画（左側・大フォント） ---
    addr_label_y = height - 50 * mm
    p.setFont(font_name, ADDR_NAME_FONT)
    p.drawString(20 * mm, addr_label_y, addressee_label)
    p.setFont(font_name, ADDR_NAME_FONT)
    addr_name_y = addr_label_y - 6 * mm
    p.drawString(20 * mm, addr_name_y, f"{addressee_name}  {addressee_suffix}")
    addr_bottom = addr_name_y

    # --- 発行者セクション: X座標の自動右寄せ計算 ---
    issuer_name = issuer_info.get('name', '')
    p.setFont(font_name, ISSUER_NAME_FONT)
    name_width = p.stringWidth(issuer_name, font_name, ISSUER_NAME_FONT)

    stamp_protrusion = STAMP_SIZE * (1 - STAMP_OVERLAP) if show_stamp else 0

    tel_text = issuer_info.get('tel', '')
    fax_text = issuer_info.get('fax', '')
    tel_fax_text = f"TEL:{tel_text}  FAX:{fax_text}" if fax_text else f"TEL:{tel_text}"
    tel_width = p.stringWidth(tel_fax_text, font_name, 9)

    max_content_width = max(name_width + stamp_protrusion, tel_width)
    issuer_x = width - RIGHT_MARGIN - max_content_width

    # --- 発行者セクション: Y座標 ---
    issuer_y = addr_bottom - LINE_GAP

    # --- 発行者セクション描画 ---
    label_font_size = ISSUER_NAME_FONT - 1
    p.setFont(font_name, label_font_size)
    p.drawString(issuer_x, issuer_y, issuer_label)
    p.setFont(font_name, ISSUER_NAME_FONT)
    p.drawString(issuer_x, issuer_y - 5 * mm, issuer_name)
    p.setFont(font_name, 9)
    postal = issuer_info.get('postal_code', '')
    if postal:
        p.drawString(issuer_x, issuer_y - 10 * mm, f"〒{postal}")
    # 住所: スペースで分割して2行表示（建物名が長い場合の折り返し）
    address = issuer_info.get('address', '')
    addr_lines = _split_address(address)
    addr_y = issuer_y - 14 * mm
    for addr_line in addr_lines:
        p.drawString(issuer_x, addr_y, addr_line)
        addr_y -= 4 * mm
    p.drawString(issuer_x, addr_y, tel_fax_text)
    issuer_height = (issuer_y - addr_y) + 4 * mm

    # --- 角印描画 ---
    stamp_bottom = issuer_y - issuer_height
    if show_stamp and os.path.exists(STAMP_PATH):
        stamp_x = issuer_x + name_width - STAMP_SIZE * STAMP_OVERLAP
        stamp_y = issuer_y - 5 * mm - STAMP_SIZE * 0.65
        p.drawImage(STAMP_PATH, stamp_x, stamp_y,
                    width=STAMP_SIZE, height=STAMP_SIZE,
                    mask='auto', preserveAspectRatio=True)
        stamp_bottom = stamp_y

    # --- 次セクション開始Y座標 ---
    issuer_text_bottom = issuer_y - issuer_height
    actual_bottom = min(issuer_text_bottom, stamp_bottom)
    next_section_y = actual_bottom - LINE_GAP

    return next_section_y


def _build_fee_table(items, font_name):
    """業務委託料金のサブテーブルを生成（EDI版 _build_fee_table() 流用）"""
    if not items:
        return "（明細行が登録されていません）"

    header = ["氏名", "基本料金（円/月）", "不足単価（円/h）", "超過単価（円/h）", "基準時間（h/月）"]
    sub_data = [header]

    for item in items:
        name = item.get('engineer_name', '作業担当者')
        sub_data.append([
            name,
            f"¥{item.get('base_fee', 0):,}",
            f"¥{item.get('deduction_rate', 0):,}",
            f"¥{item.get('overtime_rate', 0):,}",
            f"{item.get('lower_limit_hours', 0)}～{item.get('upper_limit_hours', 0)}",
        ])

    sub_data.append(["※作業報告書に基づく稼動実費精算とする。", "", "", "", ""])

    sub_table = Table(sub_data, colWidths=[38 * mm, 38 * mm, 30 * mm, 30 * mm, 38 * mm])
    sub_table.setStyle(TableStyle([
        ('FONT', (0, 0), (-1, -1), font_name, 9),
        ('FONT', (0, 0), (-1, 0), font_name, 8),
        ('GRID', (0, 0), (-1, -2), 0.3, colors.grey),
        ('BACKGROUND', (0, 0), (-1, 0), colors.Color(0.92, 0.92, 0.92)),
        ('ALIGN', (1, 0), (-1, -1), 'RIGHT'),
        ('ALIGN', (0, 0), (0, -1), 'LEFT'),
        ('VALIGN', (0, 0), (-1, -1), 'MIDDLE'),
        ('TOPPADDING', (0, 0), (-1, -1), 3),
        ('BOTTOMPADDING', (0, 0), (-1, -1), 3),
        ('LEFTPADDING', (0, 0), (-1, -1), 4),
        ('RIGHTPADDING', (0, 0), (-1, -1), 4),
        ('SPAN', (0, -1), (-1, -1)),
        ('ALIGN', (0, -1), (0, -1), 'LEFT'),
        ('FONT', (0, -1), (0, -1), font_name, 8),
        ('GRID', (0, -1), (-1, -1), 0, colors.white),
    ]))
    return sub_table


# ============================================================
# 注文書PDF（EDI: orders/services/pdf_generator.py 由来）
# ============================================================

def generate_order_pdf(data):
    """注文書PDFの生成"""
    buffer = io.BytesIO()
    p = canvas.Canvas(buffer, pagesize=A4)
    width, height = A4
    font_name = _setup_fonts(p)

    # 1. 注文番号・日付 (右上)
    p.setFont(font_name, 10)
    p.drawRightString(width - 20 * mm, height - 15 * mm, f"注文番号 : {data['order_id']}")
    p.drawRightString(width - 20 * mm, height - 20 * mm, f"{data['order_date']}")

    # 2. タイトル
    p.setFont(font_name, 20)
    p.drawCentredString(width / 2, height - 35 * mm, "注  文  書")

    # 3-6. 乙（宛先）＋甲（発行者）＋角印
    company = data.get('company', {})
    partner = data.get('partner', {})
    next_y = _draw_header_section(
        p, width, height, font_name,
        addressee_label="（乙）",
        addressee_name=partner.get('name', ''),
        addressee_suffix="御中",
        issuer_label="（甲）",
        issuer_info={
            'name': company.get('name', ''),
            'postal_code': company.get('postal_code', ''),
            'address': company.get('address', ''),
            'tel': company.get('tel', ''),
            'fax': company.get('fax', ''),
        },
        show_stamp=True,
    )

    # 7. 本文
    p.setFont(font_name, 10)
    p.drawString(20 * mm, next_y, "下記の通り注文致しますので、ご了承の上、折り返し注文請書をご送付下さい。")

    # 8. 詳細テーブル
    items = data.get('items', [])
    fee_table = _build_fee_table(items, font_name)

    kou_res = data.get('kou_responsible', company.get('responsible_person', ''))
    kou_cnt = data.get('kou_contact', company.get('contact_person', ''))
    otsu_res = data.get('otsu_responsible', partner.get('responsible_person', ''))
    otsu_cnt = data.get('otsu_contact', partner.get('contact_person', ''))

    table_data = [
        ["業務名称", data.get('project_name', '')],
        ["作業期間", f"{data.get('work_start', '')} ～ {data.get('work_end', '')}"],
        ["委託業務責任者（甲）", kou_res, "連絡窓口担当者（甲）", kou_cnt],
        ["委託業務責任者（乙）", otsu_res, "連絡窓口担当者（乙）", otsu_cnt],
        ["作業責任者", data.get('work_responsible', ''), "", ""],
        ["業務委託料金", "", "", ""],
        [fee_table, "", "", ""],
        ["作業場所", data.get('workplace', '')],
        ["納入物件", data.get('deliverable_text', '作業報告書')],
        ["支払条件", data.get('payment_condition', '月末締め翌月末払い')],
    ]

    table = Table(table_data, colWidths=[38 * mm, 52 * mm, 38 * mm, 52 * mm])
    table.setStyle(TableStyle([
        ('FONT', (0, 0), (-1, -1), font_name, 9),
        ('GRID', (0, 0), (-1, -1), 0.5, colors.black),
        ('VALIGN', (0, 0), (-1, -1), 'MIDDLE'),
        ('SPAN', (1, 0), (3, 0)),
        ('SPAN', (1, 1), (3, 1)),
        ('SPAN', (1, 4), (3, 4)),
        ('SPAN', (0, 5), (3, 5)),
        ('VALIGN', (0, 5), (0, 5), 'TOP'),
        ('ALIGN', (0, 5), (0, 5), 'LEFT'),
        ('SPAN', (0, 6), (3, 6)),
        ('TOPPADDING', (0, 6), (0, 6), 0),
        ('BOTTOMPADDING', (0, 6), (0, 6), 0),
        ('LEFTPADDING', (0, 6), (0, 6), 0),
        ('RIGHTPADDING', (0, 6), (0, 6), 0),
        ('SPAN', (1, 7), (3, 7)),
        ('SPAN', (1, 8), (3, 8)),
        ('SPAN', (1, 9), (3, 9)),
    ]))

    w, h = table.wrapOn(p, width, height)
    table_y = next_y - 5 * mm - h
    table.drawOn(p, 20 * mm, table_y)

    # 9. 契約条項
    contract_items = data.get('contract_items', '')
    if contract_items:
        p.setFont(font_name, 9)
        p.drawString(20 * mm, table_y - 10 * mm, "〈契約条項〉")
        text_obj = p.beginText(20 * mm, table_y - 15 * mm)
        text_obj.setFont(font_name, 8)
        for line in contract_items.split('\n'):
            text_obj.textLine(line)
        p.drawText(text_obj)

    p.showPage()
    p.save()
    buffer.seek(0)
    return buffer.read()


# ============================================================
# 注文請書PDF（EDI: orders/services/pdf_generator.py 由来）
# ============================================================

def generate_acceptance_pdf(data):
    """注文請書PDFの生成"""
    buffer = io.BytesIO()
    p = canvas.Canvas(buffer, pagesize=A4)
    width, height = A4
    font_name = _setup_fonts(p)

    # 1. 注文番号・日付
    p.setFont(font_name, 10)
    p.drawRightString(width - 20 * mm, height - 15 * mm, f"注文番号 : {data['order_id']}")
    p.drawRightString(width - 20 * mm, height - 20 * mm, f"{data['order_date']}")

    # 2. タイトル
    p.setFont(font_name, 20)
    p.drawCentredString(width / 2, height - 35 * mm, "注  文  請  書")

    # 3. 印紙枠
    p.rect(20 * mm, height - 40 * mm, 20 * mm, 25 * mm)
    p.setFont(font_name, 8)
    p.drawCentredString(30 * mm, height - 28 * mm, "印紙")

    # 4-5. 甲（宛先＝自社）＋乙（発行者＝パートナー）
    company = data.get('company', {})
    partner = data.get('partner', {})
    next_y = _draw_header_section(
        p, width, height, font_name,
        addressee_label="（甲）",
        addressee_name=company.get('name', ''),
        addressee_suffix="御中",
        issuer_label="（乙）",
        issuer_info={
            'name': partner.get('name', ''),
            'postal_code': partner.get('postal_code', ''),
            'address': partner.get('address', ''),
            'tel': partner.get('tel', ''),
            'fax': partner.get('fax', ''),
        },
        show_stamp=False,
    )

    # 6. テーブル
    items = data.get('items', [])
    fee_table = _build_fee_table(items, font_name)

    kou_res = data.get('kou_responsible', company.get('responsible_person', ''))
    kou_cnt = data.get('kou_contact', company.get('contact_person', ''))
    otsu_res = data.get('otsu_responsible', partner.get('responsible_person', ''))
    otsu_cnt = data.get('otsu_contact', partner.get('contact_person', ''))

    table_data = [
        ["業務名称", data.get('project_name', '')],
        ["作業期間", f"{data.get('work_start', '')} ～ {data.get('work_end', '')}"],
        ["委託業務責任者（甲）", kou_res, "連絡窓口担当者（甲）", kou_cnt],
        ["委託業務責任者（乙）", otsu_res, "連絡窓口担当者（乙）", otsu_cnt],
        ["作業責任者", data.get('work_responsible', ''), "", ""],
        ["業務委託料金", "", "", ""],
        [fee_table, "", "", ""],
        ["作業場所", data.get('workplace', '')],
        ["納入物件", data.get('deliverable_text', '作業報告書')],
        ["支払条件", data.get('payment_condition', '月末締め翌月末払い')],
    ]

    table = Table(table_data, colWidths=[38 * mm, 52 * mm, 38 * mm, 52 * mm])
    table.setStyle(TableStyle([
        ('FONT', (0, 0), (-1, -1), font_name, 9),
        ('GRID', (0, 0), (-1, -1), 0.5, colors.black),
        ('VALIGN', (0, 0), (-1, -1), 'MIDDLE'),
        ('SPAN', (1, 0), (3, 0)),
        ('SPAN', (1, 1), (3, 1)),
        ('SPAN', (1, 4), (3, 4)),
        ('SPAN', (0, 5), (3, 5)),
        ('VALIGN', (0, 5), (0, 5), 'TOP'),
        ('ALIGN', (0, 5), (0, 5), 'LEFT'),
        ('SPAN', (0, 6), (3, 6)),
        ('TOPPADDING', (0, 6), (0, 6), 0),
        ('BOTTOMPADDING', (0, 6), (0, 6), 0),
        ('LEFTPADDING', (0, 6), (0, 6), 0),
        ('RIGHTPADDING', (0, 6), (0, 6), 0),
        ('SPAN', (1, 7), (3, 7)),
        ('SPAN', (1, 8), (3, 8)),
        ('SPAN', (1, 9), (3, 9)),
    ]))

    w, h = table.wrapOn(p, width, height)
    table_y = next_y - 5 * mm - h
    table.drawOn(p, 20 * mm, table_y)

    # 7. 承諾署名欄
    p.rect(20 * mm, 20 * mm, 40 * mm, 15 * mm)
    p.drawCentredString(40 * mm, 25 * mm, "承諾署名")
    p.rect(60 * mm, 20 * mm, 130 * mm, 15 * mm)
    p.setFont(font_name, 10)
    p.drawString(65 * mm, 27 * mm, partner.get('name', ''))

    p.showPage()
    p.save()
    buffer.seek(0)
    return buffer.read()


# ============================================================
# 共通: 精算明細テーブル（イービジネス社フォーマット準拠）
# ============================================================

def _build_settlement_table(items, data, font_name):
    """精算明細テーブルを生成（イービジネス社フォーマット準拠）

    各要員につき最大4行:
    行1: 番号 | 名前 | 単価 | 作業時間 | 率 | 下限/上限 | 減 | 増 | その他 | 金額
    行2: (件名：XXX / 発注伝票番号：XXX)  ← SPANで結合
    行3: (超過金額：¥X / 控除金額：¥X)   ← SPANで結合
    行4: (下限：X時間 上限：X時間 控除単価：¥X 超過単価：¥X) ← SPANで結合
    """
    header = ["番号", "項目", "単価", "作業時間", "率", "下限/上限", "減", "増", "その他", "金額"]
    table_data = [header]

    subtotal = 0
    span_commands = []  # 動的にSPANを追加
    current_row = 1  # ヘッダーが0行目

    for i, item in enumerate(items, 1):
        name = item.get('engineer_name', '')
        base_fee = item.get('base_fee', 0)
        actual_hours = item.get('actual_hours', 0)
        effort = item.get('effort', 1.0)
        lower = item.get('lower_limit_hours', 0)
        upper = item.get('upper_limit_hours', 0)
        ded_rate = item.get('deduction_rate', 0)
        ovt_rate = item.get('overtime_rate', 0)
        excess = int(item.get('excess_amount', 0) or 0)
        shortage = int(item.get('shortage_amount', 0) or 0)
        other = int(item.get('other_amount', 0) or 0)
        project = item.get('project_name', data.get('project_name', ''))
        order_ref = item.get('order_ref', '')

        # amount（精算後金額）がある場合は常に amount − 基本単価 を正とし、
        # 「増」列と「超過金額：」テキストの不一致を防ぐ。
        # （時間×超過単価の再計算は精算方式・端数で食い違うことがある）
        if item.get('amount') is not None:
            adj = int(item.get('amount') or 0) - int(base_fee or 0)
            if adj > 0:
                excess, shortage = adj, 0
            elif adj < 0:
                excess, shortage = 0, -adj
            else:
                excess, shortage = 0, 0
        elif excess == 0 and shortage == 0 and actual_hours and (lower or upper):
            try:
                ah = float(actual_hours)
                lo = float(lower or 0)
                up = float(upper or 0)
                if up and ah > up and ovt_rate:
                    excess = int(round((ah - up) * float(ovt_rate)))
                elif lo and ah < lo and ded_rate:
                    shortage = int(round((lo - ah) * float(ded_rate)))
            except (TypeError, ValueError):
                pass

        # 精算後の確定金額（フッタ小計用）= 単価 + 増 − 減（+ その他）
        if item.get('amount') is not None:
            settled = int(item.get('amount') or 0)
        else:
            settled = int(base_fee or 0) + excess - shortage + other
        subtotal += settled

        # 金額欄: 単価 + 増（控除時は単価 − 控除）。増列は超過額のまま
        display_amount = settled

        min_max = f"{lower:.2f}/{upper:.2f}" if lower or upper else ""

        # 行1: メインデータ（減/増は超過・控除金額。単価は基本単価。金額は精算後行金額）
        table_data.append([
            str(i),
            name,
            f"{base_fee:,}",
            f"{actual_hours:.2f}" if actual_hours else "",
            f"{effort:.1f}" if effort else "1.0",
            min_max,
            f"{shortage:,}" if shortage else "",
            f"{excess:,}" if excess else "",
            f"{other:,}" if other else "",
            f"{display_amount:,}",
        ])
        main_row = current_row
        current_row += 1

        # 行2: 件名・発注伝票番号
        ref_text = f"件名：{project}"
        if order_ref:
            ref_text += f"　　　発注伝票番号：{order_ref}"
        table_data.append(["", ref_text, "", "", "", "", "", "", "", ""])
        span_commands.append(('SPAN', (1, current_row), (8, current_row)))
        current_row += 1

        # 行3: 超過/控除金額
        table_data.append([
            "", f"超過金額：¥{excess:,}　　控除金額：¥{shortage:,}",
            "", "", "", "", "", "", "", ""
        ])
        span_commands.append(('SPAN', (1, current_row), (8, current_row)))
        current_row += 1

        # 行4: 精算幅・単価の根拠
        table_data.append([
            "",
            f"下限：{lower:.2f}時間　上限：{upper:.2f}時間　控除単価：¥{ded_rate:,}　超過単価：¥{ovt_rate:,}",
            "", "", "", "", "", "", "", ""
        ])
        span_commands.append(('SPAN', (1, current_row), (8, current_row)))
        current_row += 1

    last_item_row = current_row - 1  # 最後のデータ行

    # 合計セクション（呼び出し元の保存済み金額があれば優先し、ヘッダ合計と一致させる）
    tax_rate = data.get('tax_rate', 10)
    if data.get('subtotal') is not None:
        subtotal = int(data.get('subtotal') or 0)
    if data.get('tax_amount') is not None:
        tax = int(data.get('tax_amount') or 0)
    else:
        tax = int(subtotal * tax_rate / 100)
    if data.get('total') is not None:
        total = int(data.get('total') or 0)
    else:
        total = subtotal + tax
    expenses = data.get('expenses', 0) or 0
    grand_total = total + expenses

    table_data.append(["", "", "", "", "", "", "", "", "(小計)", f"{subtotal:,}"])
    span_commands.append(('SPAN', (0, current_row), (7, current_row)))
    current_row += 1
    table_data.append(["", "", "", "", "", "", "", "", f"(消費税{tax_rate}%)", f"{tax:,}"])
    span_commands.append(('SPAN', (0, current_row), (7, current_row)))
    current_row += 1
    table_data.append(["", "", "", "", "", "", "", "", "(合計)", f"{total:,}"])
    span_commands.append(('SPAN', (0, current_row), (7, current_row)))
    current_row += 1
    if expenses:
        table_data.append(["", "", "", "", "", "", "", "", "(経費、追加)", f"{expenses:,}"])
        span_commands.append(('SPAN', (0, current_row), (7, current_row)))
        current_row += 1
    table_data.append(["", "", "", "", "", "", "", "", "(総計)", f"{grand_total:,}"])
    span_commands.append(('SPAN', (0, current_row), (7, current_row)))

    col_widths = [10*mm, 48*mm, 20*mm, 16*mm, 10*mm, 24*mm, 16*mm, 16*mm, 16*mm, 22*mm]
    table = Table(table_data, colWidths=col_widths)

    base_style = [
        ('FONT', (0, 0), (-1, -1), font_name, 7),
        ('GRID', (0, 0), (-1, last_item_row), 0.5, colors.black),
        ('BACKGROUND', (0, 0), (-1, 0), colors.Color(0.92, 0.92, 0.92)),
        ('ALIGN', (2, 0), (-1, -1), 'RIGHT'),
        ('ALIGN', (0, 0), (0, -1), 'CENTER'),
        ('ALIGN', (1, 0), (1, -1), 'LEFT'),
        ('VALIGN', (0, 0), (-1, -1), 'MIDDLE'),
        ('TOPPADDING', (0, 0), (-1, -1), 1),
        ('BOTTOMPADDING', (0, 0), (-1, -1), 1),
        ('LEFTPADDING', (0, 0), (-1, -1), 3),
        ('RIGHTPADDING', (0, 0), (-1, -1), 3),
        # 合計セクション枠
        ('BOX', (8, last_item_row + 1), (-1, -1), 0.5, colors.black),
        ('LINEABOVE', (8, last_item_row + 1), (-1, last_item_row + 1), 0.5, colors.black),
        ('LINEBELOW', (-2, -1), (-1, -1), 0.5, colors.black),
    ]

    # サブ行のフォントサイズを小さくし、背景を薄くする
    for i_item, item in enumerate(items):
        # 各要員のサブ行（行2〜4）は小さいフォント + 薄い背景
        base_row = 1 + i_item * 4  # メイン行の行番号
        for sub_offset in range(1, 4):
            sub_row = base_row + sub_offset
            base_style.append(('FONT', (0, sub_row), (-1, sub_row), font_name, 6))
            base_style.append(('BACKGROUND', (0, sub_row), (-1, sub_row), colors.Color(0.97, 0.97, 0.97)))
            base_style.append(('ALIGN', (1, sub_row), (1, sub_row), 'LEFT'))

    # SPAN追加
    base_style.extend(span_commands)

    table.setStyle(TableStyle(base_style))

    return table, grand_total



# ============================================================
# 支払通知書PDF（イービジネス社フォーマット準拠）
# ============================================================

def generate_payment_notice_pdf(data):
    """支払通知書（検収兼お支払通知書）PDFの生成

    イービジネス社のフォーマットに準拠:
    - ヘッダー: 検収番号、登録番号
    - タイトル: YYYY年MM月度 検収兼お支払通知書
    - 宛先（パートナー）殿 ＋ 発行者（自社+角印）
    - 支払方法、お支払日
    - 合計金額
    - 精算明細テーブル（10列: 番号/項目/単価/作業H/率/Min-MaxH/減/増/その他/金額）
    - 各要員の精算詳細（件名、超過/控除金額、精算幅・単価）
    """
    buffer = io.BytesIO()
    p = canvas.Canvas(buffer, pagesize=A4)
    width, height = A4
    font_name = _setup_fonts(p)

    # 1. 右上
    p.setFont(font_name, 10)
    p.drawRightString(width - 20 * mm, height - 15 * mm, f"検収番号：{data.get('notice_id', '')}")
    registration_no = data.get('registration_number', '')
    if registration_no:
        p.drawRightString(width - 20 * mm, height - 20 * mm, f"登録番号：{registration_no}")

    # 2. タイトル
    p.setFont(font_name, 16)
    target_month = data.get('target_month', '')
    p.drawCentredString(width / 2, height - 35 * mm, f"{target_month} 検収兼お支払通知書")

    # 3-4. 宛先（パートナー）殿 ＋ 発行者（自社＋角印）
    company = data.get('company', {})
    partner = data.get('partner', {})
    next_y = _draw_header_section(
        p, width, height, font_name,
        addressee_label="",
        addressee_name=partner.get('name', ''),
        addressee_suffix="殿",
        issuer_label="",
        issuer_info={
            'name': company.get('name', ''),
            'postal_code': company.get('postal_code', ''),
            'address': company.get('address', ''),
            'tel': company.get('tel', ''),
            'fax': company.get('fax', ''),
        },
        show_stamp=True,
    )

    # 5. メッセージ + 支払情報
    p.setFont(font_name, 10)
    p.drawString(20 * mm, next_y, "下記の通り、お支払いいたします。")
    info_y = next_y - 8 * mm
    p.setFont(font_name, 9)
    p.drawString(25 * mm, info_y, f"作成日付：{data.get('notice_date', '')}")
    p.drawString(25 * mm, info_y - 5 * mm, f"検収日付：{data.get('acceptance_date', data.get('notice_date', ''))}")
    p.drawString(25 * mm, info_y - 10 * mm, f"支払方法：{data.get('payment_method', '銀行振込')}")
    p.drawString(25 * mm, info_y - 15 * mm, f"お支払日：{data.get('payment_date', 'ご登録支払サイト日')}")

    # 6. 合計金額
    items = data.get('items', [])
    _, grand_total = _build_settlement_table(items, data, font_name)
    # 合計は実データから算出
    total_display = data.get('total', grand_total)

    p.setFont(font_name, 12)
    amount_y = info_y - 25 * mm
    p.rect(20 * mm, amount_y - 3 * mm, 55 * mm, 10 * mm)
    p.drawString(22 * mm, amount_y, "合計金額")
    p.rect(75 * mm, amount_y - 3 * mm, 50 * mm, 10 * mm)
    p.drawString(77 * mm, amount_y, f"¥{total_display:,}")

    # 7. 精算明細テーブル（10列）
    table, _ = _build_settlement_table(items, data, font_name)
    w, h = table.wrapOn(p, width, height)
    table_y = amount_y - 10 * mm - h
    table.drawOn(p, 5 * mm, table_y)

    p.showPage()
    p.save()
    buffer.seek(0)
    return buffer.read()


# ============================================================
# 請求書PDF（イービジネス社フォーマット準拠）
# ============================================================

def generate_invoice_pdf(data):
    """請求書PDFの生成

    2向きをサポート:
    - direction=partner_to_company（デフォルト／代理請求書）:
        宛先=company（自社）御中、発行者=partner、印影なし
    - direction=company_to_client（クライアント向け自社請求書）:
        宛先=company（クライアント）御中、発行者=partner（自社）、当社印影あり
        ※ Rust側が役割を入れ替えて渡す。振込先 bank は常に当社口座。
    """
    buffer = io.BytesIO()
    p = canvas.Canvas(buffer, pagesize=A4)
    width, height = A4
    font_name = _setup_fonts(p)

    # 1. タイトル
    p.setFont(font_name, 18)
    p.drawCentredString(width / 2, height - 25 * mm, "御 請 求 書")

    # 2. 右上情報
    p.setFont(font_name, 9)
    p.drawRightString(width - 20 * mm, height - 15 * mm, f"請求番号：{data.get('invoice_id', '')}")
    p.drawRightString(width - 20 * mm, height - 20 * mm, f"発行日：{data.get('issue_date', '')}")
    registration_no = data.get('registration_number', '')
    if registration_no:
        p.drawRightString(width - 20 * mm, height - 25 * mm, f"登録番号：{registration_no}")

    # 3-4. 宛先＋発行者
    # company = 宛先 / partner = 発行者（呼び出し側が向きに応じて役割を詰める）
    company = data.get('company', {})
    partner = data.get('partner', {})
    show_stamp = bool(data.get('show_stamp', False))
    next_y = _draw_header_section(
        p, width, height, font_name,
        addressee_label="",
        addressee_name=company.get('name', ''),
        addressee_suffix="御中",
        issuer_label="",
        issuer_info={
            'name': partner.get('name', ''),
            'postal_code': partner.get('postal_code', ''),
            'address': partner.get('address', ''),
            'tel': partner.get('tel', ''),
            'fax': partner.get('fax', ''),
            'representative': partner.get('representative', ''),
        },
        show_stamp=show_stamp,
    )

    # 5. メッセージ
    p.setFont(font_name, 10)
    p.drawString(25 * mm, next_y, "下記のとおりご請求申し上げます。")

    # 6. 御請求額
    items = data.get('items', [])
    _, grand_total = _build_settlement_table(items, data, font_name)
    total_display = data.get('total', grand_total)

    p.setFont(font_name, 12)
    amount_y = next_y - 10 * mm
    p.drawString(25 * mm, amount_y, f"御請求額：")
    p.setFont(font_name, 14)
    p.drawString(55 * mm, amount_y, f"¥{total_display:,}円")
    p.line(53 * mm, amount_y - 2 * mm, 110 * mm, amount_y - 2 * mm)

    # 7. 作業期間・支払期限・摘要
    p.setFont(font_name, 9)
    info_y = amount_y - 12 * mm
    work_period = data.get('work_period', '')
    if work_period:
        p.drawString(25 * mm, info_y, f"作業期間：{work_period}")
        info_y -= 5 * mm
    payment_deadline = data.get('payment_deadline', '')
    if payment_deadline:
        p.drawString(25 * mm, info_y, f"支払期限日：{payment_deadline}")
        info_y -= 5 * mm
    subject = data.get('subject', data.get('project_name', ''))
    if subject:
        p.drawString(25 * mm, info_y, f"摘要：{subject}")
        info_y -= 5 * mm

    # 8. 精算明細テーブル（10列）
    table, _ = _build_settlement_table(items, data, font_name)
    w, h = table.wrapOn(p, width, height)
    table_y = info_y - 5 * mm - h
    table.drawOn(p, 5 * mm, table_y)

    # 9. 振込先
    bank = data.get('bank', {})
    if bank and bank.get('bank_name'):
        bank_y = table_y - 10 * mm
        p.setFont(font_name, 9)
        p.drawString(20 * mm, bank_y, "お振込先口座：")
        p.drawString(25 * mm, bank_y - 5 * mm,
                     f"{bank.get('bank_name', '')} {bank.get('bank_branch', '')} "
                     f"({bank.get('account_type', '普通')})")
        p.drawString(25 * mm, bank_y - 10 * mm,
                     f"普通預金：{bank.get('account_number', '')}")
        p.drawString(25 * mm, bank_y - 15 * mm,
                     f"名義：{bank.get('account_name', '')}")

    p.showPage()
    p.save()
    buffer.seek(0)
    return buffer.read()


# ============================================================
# CLI エントリーポイント
# ============================================================

GENERATORS = {
    'order': generate_order_pdf,
    'acceptance': generate_acceptance_pdf,
    'invoice': generate_invoice_pdf,
    'notice': generate_payment_notice_pdf,
}


def main():
    parser = argparse.ArgumentParser(description='PDF生成スクリプト')
    parser.add_argument('--type', required=True, choices=GENERATORS.keys(),
                        help='PDF種別')
    parser.add_argument('--json', required=True,
                        help='JSONデータ')
    args = parser.parse_args()

    data = json.loads(args.json)
    pdf_bytes = GENERATORS[args.type](data)

    sys.stdout.buffer.write(pdf_bytes)


if __name__ == '__main__':
    main()
