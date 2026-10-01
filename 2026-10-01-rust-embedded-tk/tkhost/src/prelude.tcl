# tkhost prelude: evaluated once, before the UI files. The UI uses:
#
#   rs::call name ?arg ...? callback   async call of a Rust handler (on a worker
#                                      thread); on success runs
#                                      {*}$callback $result ("" = ignore it)
#   rs::cmd name ?arg ...?             sync call of the same handlers, on the Tcl
#                                      thread: returns the result or raises an
#                                      error. The UI freezes while it runs, so
#                                      keep it for quick, pure functions.
#   rs::send name ?arg ...?            fire-and-forget message to Rust
#   rs::on event cmd                   runs {*}$cmd $payload for each Rust event
#   rs::watch pattern cmd              runs {*}$cmd $key when a key of
#                                      rs::state matching `string match` changes
#   rs::state(key)                     Rust's dict (read-only mirror)
#   rs::ui(key)                        our dict: writes are sent to Rust
#
# `rs::on` and `rs::watch` replace the previous command for that event or
# pattern, so re-sourcing a UI file doesn't stack up handlers.
#
# Wire format, one message per line: Rust → Tcl lines are Tcl lists
#   set k v | unset k | event name payload | reply id ok|err result
# and Tcl → Rust lines are JSON arrays of strings
#   ["call",id,name,args...] | ["send",name,args...] | ["set",k,v] | ["unset",k]
#
# Set by Rust beforehand: rs::chan (our end of a socketpair) and rs::files,
# a list of {path embedded_source} pairs.

namespace eval rs {
    variable next_id 0
    foreach a {pending on watch mtime state ui} { variable $a; array set $a {} }

    variable jmap [list \\ \\\\ \" \\\"]
    for {set i 0} {$i < 32} {incr i} { lappend jmap [format %c $i] [format \\u%04x $i] }

    proc _send_raw {words} {
        variable chan
        variable jmap
        set quoted [lmap w $words { string cat \" [string map $jmap $w] \" }]
        puts $chan "\[[join $quoted ,]\]"
    }

    proc call {name args} {
        variable next_id
        variable pending
        if {![llength $args]} {
            return -code error {wrong # args: should be "rs::call name ?arg ...? callback"}
        }
        set id [incr next_id]
        set pending($id) [list $name [lindex $args end]]
        _send_raw [list call $id $name {*}[lrange $args 0 end-1]]
    }
    proc send {name args} { _send_raw [list send $name {*}$args] }
    proc on {event cmd} { variable on; set on($event) $cmd }
    proc watch {pattern cmd} { variable watch; set watch($pattern) $cmd }

    proc _dispatch {line} {
        variable on
        variable pending
        set rest [lassign $line kind]
        switch -- $kind {
            set   { lassign $rest k v; set ::rs::state($k) $v }
            unset { unset -nocomplain ::rs::state([lindex $rest 0]) }
            event {
                lassign $rest name payload
                if {[info exists on($name)]} { uplevel #0 [list {*}$on($name) $payload] }
            }
            reply {
                lassign $rest id status result
                lassign $pending($id) name cb
                unset pending($id)
                if {$status ne "ok"} {
                    puts stderr "rs::call $name: $result"
                } elseif {$cb ne ""} {
                    uplevel #0 [list {*}$cb $result]
                }
            }
            default { error "unknown message from rust: $line" }
        }
    }

    proc _readable {} {
        variable chan
        while {[gets $chan line] >= 0} {
            if {[catch {_dispatch $line}]} { puts stderr $::errorInfo }
        }
        if {[eof $chan]} { exit 0 }
    }

    proc _on_state {name1 key op} {
        variable watch
        foreach {pat cmd} [array get watch] {
            if {[string match $pat $key]} { uplevel #0 [list {*}$cmd $key] }
        }
    }
    proc _on_ui {name1 key op} {
        if {$key eq ""} return
        if {$op eq "write"} {
            _send_raw [list set $key $::rs::ui($key)]
        } else {
            _send_raw [list unset $key]
        }
    }

    # source file $i from disk if its mtime changed (always if $force),
    # falling back to the embedded copy if that fails
    proc _load {i force} {
        variable files
        variable mtime
        lassign [lindex $files $i] path data
        if {[catch {file mtime $path} mt]} { set mt "" }
        if {!$force && [info exists mtime($i)] && $mt eq $mtime($i)} return
        set mtime($i) $mt
        if {$mt ne ""} {
            if {![catch {uplevel #0 [list source -encoding utf-8 $path]}]} {
                puts stderr "loaded $path"
                return
            }
            puts stderr "error in $path: $::errorInfo"
        }
        puts stderr "using embedded copy of $path"
        uplevel #0 $data
    }

    proc _reload_loop {} {
        variable files
        for {set i 0} {$i < [llength $files]} {incr i} {
            if {[catch {_load $i 0}]} { puts stderr $::errorInfo }
        }
        after 2000 rs::_reload_loop
    }

    trace add variable ::rs::state {write unset} rs::_on_state
    trace add variable ::rs::ui {write unset} rs::_on_ui
    interp bgerror {} {apply {{msg opts} { puts stderr [dict get $opts -errorinfo] }}}

    fconfigure $chan -blocking 0 -buffering line -encoding utf-8 -translation lf
    # whatever Rust sent before the UI exists (e.g. initial rs::state)
    _readable
    fileevent $chan readable rs::_readable
    for {set i 0} {$i < [llength $files]} {incr i} {
        if {[catch {_load $i 1}]} { puts stderr $::errorInfo }
    }
    after 2000 rs::_reload_loop
}
