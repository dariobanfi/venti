// The pilot's choice of light or dark, applied before the page is first
// drawn, so that a chosen theme does not flash the system's. A file of the
// app, because the page's policy runs no script written into it. It does
// what `applyTheme` in src/lib/app/theme.ts does, with the same key.
(function () {
	var theme;
	try {
		theme = localStorage.getItem('venti.theme');
	} catch (error) {
		return;
	}
	if (theme !== 'light' && theme !== 'dark') return;
	document.documentElement.dataset.theme = theme;
	var metas = document.querySelectorAll('meta[name="theme-color"]');
	for (var i = 0; i < metas.length; i++) {
		metas[i].media = metas[i].dataset.scheme === theme ? 'all' : 'not all';
	}
})();
