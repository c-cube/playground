catch {destroy .f}
pack [ttk::frame .f -padding 8] -fill both -expand 1
ttk::label .f.greet -textvariable rs::state(greeting)
ttk::label .f.ticks -textvariable rs::state(ticks)
ttk::label .f.count -text "count: 0"
ttk::button .f.add -text "+1" -command { rs::call add 1 {apply {{n} { .f.count configure -text "count: $n" }}} }
ttk::button .f.fail -text "fail" -command { rs::call fail {} }
ttk::entry .f.name -textvariable rs::ui(name)
# sync: same handlers as rs::call, result returned directly
ttk::label .f.upper -text [rs::cmd upper "sync" "call"]
pack {*}[winfo children .f] -anchor w -pady 2

rs::on tick {apply {{p} {
    if {[dict get $p i] == 1} { rs::send log [dict get $p words] }
}}}
rs::watch tick* {apply {{k} { wm title . "hello — $k=$rs::state($k)" }}}
rs::call fail {}   ;# prints "rs::call fail: this one always fails"
if {[catch {rs::cmd fail} err]} { puts stderr "rs::cmd fail raised: $err" }
