/* ========================================
   Sophia — メインJavaScript
   ======================================== */

document.addEventListener('DOMContentLoaded', function() {

    // ── サイドバートグル ──
    const sidebarToggle = document.getElementById('sidebarToggle');
    const sidebar = document.getElementById('sidebar');

    if (sidebarToggle && sidebar) {
        sidebarToggle.addEventListener('click', function() {
            // モバイルではshow/hide、デスクトップではcollapse
            if (window.innerWidth < 992) {
                sidebar.classList.toggle('show');
            } else {
                sidebar.classList.toggle('collapsed');
            }
        });

        // モバイルでサイドバー外をクリックしたら閉じる
        document.addEventListener('click', function(e) {
            if (window.innerWidth < 992 &&
                sidebar.classList.contains('show') &&
                !sidebar.contains(e.target) &&
                !sidebarToggle.contains(e.target)) {
                sidebar.classList.remove('show');
            }
        });
    }

    // ── 確認モーダル (data-confirm 属性) ──
    const confirmModal = document.getElementById('confirmModal');
    if (confirmModal) {
        const modal = new bootstrap.Modal(confirmModal);
        const modalBody = document.getElementById('confirmModalBody');
        const modalTitle = document.getElementById('confirmModalTitle');
        const modalOk = document.getElementById('confirmModalOk');
        let pendingAction = null;

        document.addEventListener('click', function(e) {
            const trigger = e.target.closest('[data-confirm]');
            if (!trigger) return;

            e.preventDefault();
            e.stopPropagation();

            const message = trigger.dataset.confirm;
            const title = trigger.dataset.confirmTitle || '確認';
            const okText = trigger.dataset.confirmOk || 'OK';
            const style = trigger.dataset.confirmStyle || 'primary';

            modalBody.textContent = message;
            modalTitle.innerHTML = '<i class="bi bi-question-circle text-' + style + ' me-2"></i>' + title;
            modalOk.className = 'btn btn-' + style;
            modalOk.textContent = okText;

            pendingAction = trigger;
            modal.show();
        });

        if (modalOk) {
            modalOk.addEventListener('click', function() {
                modal.hide();
                if (!pendingAction) return;

                // formのsubmit or aのクリック
                const form = pendingAction.closest('form');
                if (pendingAction.tagName === 'A') {
                    window.location.href = pendingAction.href;
                } else if (form && (pendingAction.type === 'submit' || form.dataset.confirm)) {
                    form.removeAttribute('data-confirm');
                    form.submit();
                } else if (pendingAction.tagName === 'BUTTON' || pendingAction.tagName === 'INPUT') {
                    pendingAction.removeAttribute('data-confirm');
                    pendingAction.click();
                }
                pendingAction = null;
            });
        }
    }

    // ── ツールチップ初期化 ──
    const tooltips = document.querySelectorAll('[data-bs-toggle="tooltip"]');
    tooltips.forEach(function(el) {
        new bootstrap.Tooltip(el);
    });
});
