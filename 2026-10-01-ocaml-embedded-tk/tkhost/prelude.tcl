# tkhost prelude: evaluated once, before the UI files. The UI uses:
#
#   ocaml::NAME ?arg ...?           sync primitive: runs on the Tcl thread,
#                                   returns the result or raises an error. The
#                                   UI freezes while it runs.
#   ocaml::NAME ?arg ...? callback  async primitive: returns at once; on success
#                                   runs {*}$callback $result ("" = ignore it),
#                                   errors go to stderr
#   ocaml::on event cmd             runs {*}$cmd $payload for each OCaml event
#   ocaml::watch pattern cmd        runs {*}$cmd $key when a key of ocaml::state
#                                   matching `string match` changes
#   ocaml::state(key)               OCaml's dict (read-only)
#   ocaml::pstate(key)              OCaml's persisted dict (read-only)
#
# `ocaml::on` and `ocaml::watch` replace the previous command for that event or
# pattern, so re-sourcing a UI file doesn't stack up handlers.
#
# Everything OCaml sends goes through one queue, drained every 10ms by
# `ocaml::_cmd poll`: one Tcl list per line,
#   set state|pstate k v | unset state|pstate k | event name payload
#   | reply id ok|err result | define name sync|async
#
# Set by OCaml beforehand: the ocaml::_cmd command, and ocaml::files, a list of
# {path embedded_source} pairs.

namespace eval ocaml {
    variable next_id 0
    foreach a {pending on watch mtime state pstate} { variable $a; array set $a {} }

    proc _async {name args} {
        variable next_id
        variable pending
        if {![llength $args]} {
            return -code error "wrong # args: should be \"ocaml::$name ?arg ...? callback\""
        }
        set id [incr next_id]
        set pending($id) [list $name [lindex $args end]]
        _cmd start $id $name {*}[lrange $args 0 end-1]
        return
    }

    proc on {event cmd} { variable on; set on($event) $cmd }
    proc watch {pattern cmd} { variable watch; set watch($pattern) $cmd }

    proc _dispatch {line} {
        variable on
        variable pending
        set rest [lassign $line kind]
        switch -- $kind {
            set   { lassign $rest arr k v; set ::ocaml::${arr}($k) $v }
            unset { lassign $rest arr k; unset -nocomplain ::ocaml::${arr}($k) }
            event {
                lassign $rest name payload
                if {[info exists on($name)]} { uplevel #0 [list {*}$on($name) $payload] }
            }
            reply {
                lassign $rest id status result
                lassign $pending($id) name cb
                unset pending($id)
                if {$status ne "ok"} {
                    puts stderr "ocaml::$name: $result"
                } elseif {$cb ne ""} {
                    uplevel #0 [list {*}$cb $result]
                }
            }
            define {
                lassign $rest name mode
                if {$mode eq "sync"} {
                    interp alias {} ::ocaml::$name {} ::ocaml::_cmd call $name
                } else {
                    interp alias {} ::ocaml::$name {} ::ocaml::_async $name
                }
            }
            default { error "unknown message from OCaml: $line" }
        }
    }

    proc _poll {} {
        foreach line [split [_cmd poll] \n] {
            if {$line ne "" && [catch {_dispatch $line}]} { puts stderr $::errorInfo }
        }
        after 10 ocaml::_poll
    }

    proc _on_state {name1 key op} {
        variable watch
        foreach {pat cmd} [array get watch] {
            if {[string match $pat $key]} { uplevel #0 [list {*}$cmd $key] }
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

    proc _reload {{force 0}} {
        variable files
        for {set i 0} {$i < [llength $files]} {incr i} {
            if {[catch {_load $i $force}]} { puts stderr $::errorInfo }
        }
        after 2000 ocaml::_reload
    }

    trace add variable ::ocaml::state {write unset} ocaml::_on_state
    interp bgerror {} {apply {{msg opts} { puts stderr [dict get $opts -errorinfo] }}}

    # primitives and state OCaml set up before the UI exists, then the UI
    _poll
    _reload 1
}
