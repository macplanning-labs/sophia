/**
 * dashboard_pipeline.js — ダッシュボードのパイプライン操作
 * 行クリック → インラインステータス変更パネル
 */
(function() {
    var PURCHASE_STATUSES = [
        { value: 'DRAFT', label: '注文書作成' },
        { value: 'SENT', label: '注文書送付' },
        { value: 'ACCEPTED', label: 'パートナー承諾' },
        { value: 'REPORT_RECEIVED', label: '報告書受領' },
        { value: 'NOTICE_CREATED', label: '支払通知書作成' },
        { value: 'NOTICE_CONFIRMED', label: '受諾' },
        { value: 'PAID', label: '支払完了' }
    ];
    var RECEIVED_STATUSES = [
        { value: 'REGISTERED', label: '受注登録' },
        { value: 'REPORT_RECEIVED', label: '勤怠受領' },
        { value: 'REPORT_SENT', label: '報告書送付' },
        { value: 'INVOICED', label: '請求書処理' },
        { value: 'PAID', label: '入金確認' }
    ];

    function getStatusIndex(statuses, value) {
        return statuses.findIndex(function(s) { return s.value === value; });
    }

    document.querySelectorAll('.wf-clickable-row').forEach(function(row) {
        row.addEventListener('click', function() {
            var orderId = this.dataset.orderId;
            var orderType = this.dataset.orderType;
            var currentStatus = this.dataset.status;
            var entity = this.dataset.entity;
            var project = this.dataset.project;
            var month = this.dataset.month;

            if (this.classList.contains('wf-row-selected')) {
                closeDetail();
                return;
            }
            closeDetail();
            this.classList.add('wf-row-selected');

            var statuses = orderType === 'purchase' ? PURCHASE_STATUSES : RECEIVED_STATUSES;
            var currentIdx = getStatusIndex(statuses, currentStatus);

            var detailRow = document.createElement('tr');
            detailRow.className = 'wf-detail-row';
            var colCount = this.children.length;
            var td = document.createElement('td');
            td.setAttribute('colspan', colCount);

            var html = '<div class="wf-detail-panel">';
            html += '<div class="wf-detail-header">';
            html += '<div class="wf-detail-title">' + entity + ' / ' + project + ' — ' + month + '</div>';
            html += '<div class="d-flex gap-2">';
            if (orderType === 'purchase') {
                html += '<a href="/orders/' + orderId + '" class="btn btn-sm btn-outline-primary">📝 詳細</a>';
            } else {
                html += '<a href="/received-orders/' + orderId + '" class="btn btn-sm btn-outline-primary">📝 詳細</a>';
            }
            html += '<button type="button" class="btn btn-sm btn-outline-secondary" onclick="closeDetail()">✕</button>';
            html += '</div></div>';
            html += '<div class="wf-step-bar">';

            statuses.forEach(function(s, i) {
                var stepClass = 'step-pending';
                var icon = '○';
                var action = 'クリックで設定';
                if (i <= currentIdx) {
                    stepClass = 'step-done';
                    icon = '✓';
                    action = '完了';
                } else if (i === currentIdx + 1) {
                    stepClass = 'step-active';
                    icon = '●';
                    action = '▶ クリックで進める';
                }
                html += '<div class="wf-step-item ' + stepClass + '" '
                    + 'data-status="' + s.value + '" '
                    + 'data-order-id="' + orderId + '" '
                    + 'data-order-type="' + orderType + '" '
                    + 'onclick="changeStatus(this)">';
                html += '<div class="wf-step-label">' + icon + ' ' + s.label + '</div>';
                html += '<div class="wf-step-action">' + action + '</div>';
                html += '</div>';
            });

            html += '</div></div>';
            td.innerHTML = html;
            detailRow.appendChild(td);
            this.parentNode.insertBefore(detailRow, this.nextSibling);
        });
    });

    window.closeDetail = function() {
        document.querySelectorAll('.wf-detail-row').forEach(function(el) { el.remove(); });
        document.querySelectorAll('.wf-row-selected').forEach(function(el) { el.classList.remove('wf-row-selected'); });
    };

    window.changeStatus = function(stepEl) {
        var newStatus = stepEl.dataset.status;
        var orderId = stepEl.dataset.orderId;
        var orderType = stepEl.dataset.orderType;

        var statuses = orderType === 'purchase' ? PURCHASE_STATUSES : RECEIVED_STATUSES;
        var found = statuses.find(function(s) { return s.value === newStatus; });
        var statusLabel = found ? found.label : newStatus;

        if (!confirm('ステータスを「' + statusLabel + '」に変更しますか？')) return;

        var url = orderType === 'purchase'
            ? '/api/orders/' + orderId + '/status'
            : '/api/received-orders/' + orderId + '/status';

        fetch(url, {
            method: 'POST',
            headers: { 'Content-Type': 'application/json' },
            body: JSON.stringify({ status: newStatus })
        })
        .then(function(r) { return r.json(); })
        .then(function(data) {
            if (data.ok) { location.reload(); }
            else { alert('エラー: ' + (data.error || '不明')); }
        })
        .catch(function() { alert('通信エラーが発生しました'); });
    };
})();
