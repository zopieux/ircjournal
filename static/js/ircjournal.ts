function prepare_highlight() {
    document.querySelector(".messages").classList.add("highlight");
    // document.querySelector("#clear-selection").classList.add("active");
    document.querySelectorAll(".messages .msg.highlight").forEach(e => e.classList.remove("highlight"));
}

function clear_highlight() {
    document.querySelector(".messages").classList.remove("highlight");
    // document.querySelector("#clear-selection").classList.remove("active");
    document.querySelectorAll(".messages .msg.highlight").forEach(e => e.classList.remove("highlight"));
}

function highlight_single_line(id_or_ts: string): HTMLElement {
    let elem;
    const by_id = document.getElementById(id_or_ts);
    if (by_id) {
        elem = by_id
    } else {
        elem = document.querySelector(`[data-timestamp="${id_or_ts}"]`)
    }
    if (elem) elem.classList.add("highlight")
    return elem
}

function highlight_lines(range_of_ts: string[]): HTMLElement {
    const [first, last] = range_of_ts.map(e => parseInt(e, 10)).sort()
    let elem = null;
    document.querySelectorAll(".messages .msg").forEach((e: HTMLElement) => {
        const ts = parseInt(e.dataset.timestamp, 10)
        if (first <= ts && ts <= last) {
            if (!elem) elem = e
            e.classList.add("highlight")
        }
    })
    return elem
}

function scroll_to(elem) {
    elem.scrollIntoView()
}

function first_selection_target(): string {
    const current = window.location.hash.substring(1).split(';')
    const [selection, filter] = current
    if (selection == null || selection == "") return null
    const range = selection.split("-")
    return range[0]
}


function by_id_or_ts(id_or_ts: string): [HTMLElement, string] {
    const by_id = document.getElementById(id_or_ts)
    if (by_id) return [by_id, id_or_ts]
    const by_ts = document.querySelector(`.msg[data-timestamp="${id_or_ts}"]`) as HTMLElement
    if (by_ts) return [by_ts, by_ts.dataset.timestamp]
}

function on_hash_change() {
    const current = window.location.hash.substring(1).split(';')
    const [selection, filter] = current;
    if (selection != null) {
        if (selection == "") {
            clear_highlight();
        } else {
            prepare_highlight();
            const range = selection.split("-")
            if (range.length == 1) {
                highlight_single_line(range[0])
            } else {
                highlight_lines(range)
            }
        }
    }
}

function set_hash(selection: string, filter: string) {
    let [curr_selection, curr_filter] = window.location.hash.substring(1).split(";")
    if (selection != null) curr_selection = selection
    if (filter != null) curr_filter = filter
    window.location.hash = `#${curr_selection || ""};${curr_filter || ""}`
}

function tslink_click(e: Event, shift_pressed: boolean) {
    e.preventDefault()
    const msg = (e.target as HTMLElement).parentElement.parentElement.parentElement

    if (shift_pressed) {
        const to_ts = msg.dataset.timestamp;
        let from_target = first_selection_target()
        const [from_elem, _] = by_id_or_ts(from_target)
        if (from_elem) from_target = from_elem.dataset.timestamp
        set_hash(`${from_target}-${to_ts}`, null)
    } else {
        set_hash(msg.id, null)
    }
}

function app() {
    let shift_pressed = false;
    document.addEventListener('keydown', (e) => {
        if (e.key == "Shift") shift_pressed = true
    })
    document.addEventListener('keyup', (e) => {
        if (e.key == "Shift") shift_pressed = false
    })
    window.addEventListener('hashchange', () => on_hash_change())
    document.querySelectorAll("a.tslink[href^='#']").forEach(
        e => e.addEventListener("click", e => tslink_click(e, shift_pressed)));

    on_hash_change()

    const [highlighted, _] = by_id_or_ts(first_selection_target())
    if (highlighted) setTimeout(() => scroll_to(highlighted), 250)
}

(function ready(fn) {
    if (document.readyState != 'loading') {
        fn()
    } else {
        document.addEventListener('DOMContentLoaded', fn);
    }
})(app)
