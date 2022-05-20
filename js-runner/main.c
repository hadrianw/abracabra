#if 0
set -e; [ "$0" -nt "$0.bin" -o "$HOME/repos/quickjs/libquickjs.lto.a" -nt "$0.bin" ] &&
gcc -Wall -Wextra -std=c11 -g -O2 -flto -I$HOME/repos/quickjs -L$HOME/repos/quickjs "$0" -lquickjs.lto -ldl -lm -o "$0.bin"
echo done
exec "$0.bin" "$@"
#endif

#include <quickjs.h>
#include <quickjs-libc.h>

#include <unistd.h>
#include <stdalign.h>
#include <stddef.h>
#include <string.h>

// 64 bit system: sizeof(struct alloc_head) == sizeof(size_t)
struct alloc_head {
	// sizes in sizeof(size_t)
	uint32_t self;
	uint32_t prev;
};

static size_t
self_size(const struct alloc_head *h) {
	return h->self * sizeof(size_t);
}

// not sure how big it should be, but 640k ought to be enough for anybody
char jsheap[640 * 1024] __attribute__ ((aligned (alignof(size_t))));
void *jsheap_last = NULL;

static size_t
aligned_size(size_t size)
{
	return (size + alignof(size_t) - 1) / alignof(size_t) * alignof(size_t);
}

static size_t
aa_js_malloc_usable_size(const void *ptr)
{
	const struct alloc_head *p = ptr;
	const struct alloc_head *h = p - 1;
	return self_size(h);
}

static void*
aa_js_malloc(JSMallocState *s, size_t size)
{
	size_t asize = aligned_size(size);
	size_t masize = sizeof(struct alloc_head) + asize;

	if(s->malloc_size + masize > s->malloc_limit) {
		fputs("malloc: malloc_limit reached\n" , stderr);
		return NULL;
	}
	if(masize > sizeof(jsheap) - s->malloc_size) {
		fputs("malloc: no space left\n" , stderr);
		return NULL;
	}
	struct alloc_head *h = (struct alloc_head*)&jsheap[s->malloc_size];
	h->self = asize / sizeof(size_t);
	if(jsheap_last != NULL) {
		h->prev = aa_js_malloc_usable_size(jsheap_last) / sizeof(size_t);
	} else {
		h->prev = 0;
	}

	s->malloc_count += 1;
	s->malloc_size += masize;
	jsheap_last = h + 1;
	return jsheap_last;
}

static void
aa_js_free(JSMallocState *s, void *ptr)
{
	if(ptr == NULL) {
		return;
	}

	if(ptr == jsheap_last) {
		struct alloc_head *p = ptr;
		struct alloc_head *h = p - 1;

		if(jsheap_last == jsheap + sizeof(struct alloc_head)) {
			jsheap_last = NULL;
		} else {
			jsheap_last = h - h->prev;
		}
		s->malloc_size -= sizeof(struct alloc_head) + self_size(h);
	}

	s->malloc_count -= 1;
}

static void*
aa_js_realloc(JSMallocState *s, void *ptr, size_t size)
{
	if(ptr == NULL) {
		if(size == 0) {
			return NULL;
		}
		return aa_js_malloc(s, size);
	}

	if (size == 0) {
		aa_js_free(s, ptr);
		return NULL;
	}

	size_t asize = aligned_size(size);
	size_t masize = sizeof(struct alloc_head) + asize;
	struct alloc_head *p = ptr;
	struct alloc_head *h = p - 1;
	if(s->malloc_size + masize - self_size(h) > s->malloc_limit) {
		fputs("realloc: malloc_limit reached\n" , stderr);
		return NULL;
	}

	aa_js_free(s, ptr);
	void *newptr = aa_js_malloc(s, size);
	if(newptr != ptr) {
		memcpy(newptr, ptr, self_size(h));
	}
	return newptr;
}

static JSValue
insertBefore(JSContext *ctx, JSValueConst this_val, int argc, JSValueConst *argv)
{
	JSValue el = argv[0];
	JSValue src = JS_GetPropertyStr(ctx, el, "src");
	if (JS_IsException(src)) {
		return JS_EXCEPTION;
	}
	const char *str = JS_ToCString(ctx, src);
        if (str == NULL) {
		return JS_EXCEPTION;
        }
        printf("src: %s\n", str);
        JS_FreeCString(ctx, str);
        JS_FreeValue(ctx, src);

	return JS_UNDEFINED;
}

static JSValue
getElementsByTagName(JSContext *ctx, JSValueConst this_val, int argc, JSValueConst *argv)
{
	JSValue arr = JS_NewArray(ctx);
	if (JS_IsException(arr)) {
		return JS_EXCEPTION;
	}

	JSValue elem = JS_NewObject(ctx);
	if (JS_IsException(elem)) {
		// FIXME: leaks arr
		return JS_EXCEPTION;
	}
	JS_DefinePropertyValueUint32(ctx, arr, 0, elem, JS_PROP_C_W_E);

	JSValue parent = JS_NewObject(ctx);
	if (JS_IsException(elem)) {
		// FIXME: leaks arr and elem
		return JS_EXCEPTION;
	}
	JS_DefinePropertyValueStr(ctx, elem, "parentNode", parent, JS_PROP_C_W_E);

	JS_SetPropertyStr(ctx, parent, "insertBefore", JS_NewCFunction(ctx, insertBefore, "createElement", 1));
	
	return arr;
}

static JSValue
createElement(JSContext *ctx, JSValueConst this_val, int argc, JSValueConst *argv)
{
	return JS_NewObject(ctx);
}

int
loop(void)
{
	int rc = 1;

	///*
	JSRuntime *rt = JS_NewRuntime2(&(JSMallocFunctions){
		.js_malloc = aa_js_malloc,
		.js_free = aa_js_free,
		.js_realloc = aa_js_realloc,
		.js_malloc_usable_size = aa_js_malloc_usable_size,
	}, NULL);
	//*/JSRuntime *rt = JS_NewRuntime();
	if(rt == NULL) {
		fputs("JS_NewRuntime2 failed\n", stderr);
		return 1;
	}

	js_std_init_handlers(rt);
	JSContext *ctx = JS_NewContext(rt);
	if(rt == NULL) {
		fputs("JS_NewContext failed\n", stderr);
		goto out_runtime;
	}

	js_std_add_helpers(ctx, 0, 0);

	JSValue global_obj = JS_GetGlobalObject(ctx);

	JSValue window = JS_NewObject(ctx);
	rc = JS_SetPropertyStr(ctx, global_obj, "window", window);
	js_std_dump_error(ctx);

	JSValue document = JS_NewObject(ctx);
	JS_SetPropertyStr(ctx, document, "getElementsByTagName", JS_NewCFunction(ctx, getElementsByTagName, "getElementsByTagName", 1));
	JS_SetPropertyStr(ctx, document, "createElement", JS_NewCFunction(ctx, createElement, "createElement", 1));
	JS_SetPropertyStr(ctx, global_obj, "document", document);
	JS_FreeValue(ctx, global_obj);

	char buf[] =
	"(function(w,d,s,l,i){\n"
		"w[l]=w[l]||[];\n"
		"w[l].push({\n"
			"'gtm.start': new Date().getTime(),\n"
			"event:'gtm.js'\n"
		"});\n"
		"var f=d.getElementsByTagName(s)[0],\n"
			"j=d.createElement(s),\n"
			"dl=l!='dataLayer'?'&l='+l:'';\n"
		"j.async=true;\n"
		"j.src='https://www.googletagmanager.com/gtm.js?ver=5.7.1&id='+i+dl;\n"
		"f.parentNode.insertBefore(j,f);\n"
	"})(window,document,'script','dataLayer','GTM-KQRQVWK');\n";

	JSValue val = JS_Eval(ctx, buf, sizeof(buf)-1, "<url>", JS_EVAL_TYPE_GLOBAL);
	puts("after eval");
	if(JS_IsException(val) != 0) {
		js_std_dump_error(ctx);
	}
	js_std_dump_error(ctx);

	/*
	JSMemoryUsage stats;
	JS_ComputeMemoryUsage(rt, &stats);
	JS_DumpMemoryUsage(stdout, &stats, rt);
	*/

	JS_FreeValue(ctx, val);

	rc = 0;
	js_std_free_handlers(rt);
out_context:
	JS_FreeContext(ctx);
out_runtime:
	JS_FreeRuntime(rt);

	return rc;
}

int
main(int argc, char **argv)
{
	(void)argc; (void)argv;

	for(int i = 0; i < 10000; i++) {
		loop();
		jsheap_last = NULL;
	}
	return 0;
}
