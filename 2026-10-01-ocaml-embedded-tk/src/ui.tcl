# tkchat UI, hosted by tkhost (see tkhost/prelude.tcl for the ocaml:: API).
# Re-sourced whenever this file changes, so it must be idempotent:
# everything lives under .main, state lives in the ui namespace.
#
# The ocaml:: primitives, events and state it uses are listed in src/gui.ml;
# ocaml::pstate holds cur and joined.
#
# Presence: every 5s we announce the channels we joined; authors who haven't
# announced themselves for 40s are gone.

catch {destroy .main}

namespace eval ui {
    if {![info exists cur]} {
        variable cur [expr {[info exists ::ocaml::pstate(cur)] ? $::ocaml::pstate(cur) : "general"}]
    }
    if {![info exists joined]} {
        variable joined [expr {[info exists ::ocaml::pstate(joined)] ? $::ocaml::pstate(joined) : [list general]}]
    }
    if {![info exists known]} { variable known {} }  ;# channels the daemon has
    # oldest, newest: smallest and largest message id shown in the log
    variable shown {}    ;# channel names, in tree order
    variable unread; array set unread {}
    variable seen; array set seen {}  ;# author -> {last_announce_time chans}
    variable tick        ;# the pending `after` for ui::tick
    if {[info exists tick]} { after cancel $tick }
}

wm geometry . 800x500

ttk::frame .main
pack .main -fill both -expand 1

# connection status, top right
ttk::frame .main.top -padding {4 2 6 0}
ttk::label .main.top.status -compound left
pack .main.top.status -side right
pack .main.top -fill x

ttk::panedwindow .main.pw -orient horizontal
pack .main.pw -fill both -expand 1

# --- channel list ---
set L [ttk::frame .main.pw.left -padding 4]
ttk::label $L.title -text "Channels" -font TkHeadingFont
ttk::treeview $L.chans -show tree -selectmode browse
ttk::entry $L.new
ttk::button $L.join -text "Join" -command ui::join_new
grid $L.title -columnspan 2 -sticky w
grid $L.chans -columnspan 2 -sticky nsew
grid $L.new $L.join -sticky ew -pady {4 0}
grid rowconfigure $L 1 -weight 1
grid columnconfigure $L 0 -weight 1
bind $L.chans <<TreeviewSelect>> ui::on_select
$L.chans tag configure unread -font TkHeadingFont

# red dot for channels with unread messages, transparent one otherwise
# (also the connection status: green when connected, red when not)
foreach {img color} {ui::dot red ui::nodot "" ui::green green3} {
    catch {image delete $img}
    image create photo $img -width 10 -height 10
    if {$color eq ""} continue
    for {set x 0} {$x < 10} {incr x} {
        for {set y 0} {$y < 10} {incr y} {
            if {($x-4.5)**2 + ($y-4.5)**2 <= 16} { $img put $color -to $x $y }
        }
    }
}
bind $L.new <Return> ui::join_new

# --- messages ---
set R [ttk::frame .main.pw.right -padding 4]
ttk::button $R.older -text "Load older" -command ui::load_older
text $R.log -state disabled -wrap word -yscrollcommand [list $R.sb set] \
    -font TkDefaultFont -padx 6 -pady 4
ttk::scrollbar $R.sb -command [list $R.log yview]
ttk::entry $R.input
ttk::button $R.send -text "Send" -command ui::send
grid $R.older - -sticky w -pady {0 4}
grid $R.log $R.sb -sticky nsew
grid $R.input $R.send -sticky ew -pady {4 0}
grid rowconfigure $R 1 -weight 1
grid columnconfigure $R 0 -weight 1
bind $R.input <Return> ui::send
# toplevel bindings apply to every widget in the window
bind . <Alt-Up>   {ui::move_chan -1}
bind . <Alt-Down> {ui::move_chan 1}

$R.log tag configure ts -foreground gray50

# --- who's here ---
set U [ttk::frame .main.pw.users -padding 4]
ttk::label $U.title -text "Here" -font TkHeadingFont
listbox $U.list -width 18 -activestyle none -takefocus 0
grid $U.title -sticky w
grid $U.list -sticky nsew
grid rowconfigure $U 1 -weight 1
grid columnconfigure $U 0 -weight 1

.main.pw add $L -weight 0
.main.pw add $R -weight 1
.main.pw add $U -weight 0

namespace eval ui {
    proc log {} { return .main.pw.right.log }
    proc me {} { return $::ocaml::state(author) }

    proc update_status {args} {
        wm title . "tkchat-ml — [me]"
        if {$::ocaml::state(connected)} {
            set img ui::green
            set text " connected"
        } else {
            set img ui::dot
            set text " disconnected"
        }
        set q $::ocaml::state(outbox)
        if {$q > 0} { append text " ($q queued)" }
        .main.top.status configure -image $img -text $text
    }

    proc fetch_chans {} { ocaml::channels ui::got_chans }
    proc got_chans {chans} {
        variable known $chans
        refresh_chans
    }

    proc refresh_chans {} {
        variable cur
        variable known
        variable joined
        variable shown
        variable unread
        set shown [lsort -unique [concat $known $joined]]
        set tv .main.pw.left.chans
        $tv delete [$tv children {}]
        set i 0
        foreach c $shown {
            set n [expr {[info exists unread($c)] ? $unread($c) : 0}]
            if {$n > 0} {
                set opts [list -text " # $c  ($n)" -image ui::dot -tags unread]
            } else {
                set opts [list -text " # $c" -image ui::nodot]
            }
            $tv insert {} end -id c$i {*}$opts
            incr i
        }
        set i [lsearch -exact $shown $cur]
        if {$i >= 0} { $tv selection set c$i; $tv see c$i }
    }

    # an author's color: ours, or one picked by OCaml
    proc color {who} {
        expr {$who eq [me] ? "DarkGreen" : [ocaml::author_color $who]}
    }

    # text tag for an author's name
    proc author_tag {who} {
        set t [log]
        if {"a:$who" ni [$t tag names]} {
            $t tag configure a:$who -font TkHeadingFont -foreground [color $who]
        }
        return a:$who
    }

    # insert one message at $where ("end" or "1.0")
    proc render {m where} {
        set t [log]
        set ts [clock format [expr {[dict get $m ts] / 1000}] -format %H:%M:%S]
        set who [dict get $m author]
        set tag [author_tag $who]
        $t configure -state normal
        $t insert $where "$ts " ts "<$who> " [list $tag] "[dict get $m msg]\n" {}
        $t configure -state disabled
    }

    proc clear_log {} {
        set t [log]
        $t configure -state normal
        $t delete 1.0 end
        $t configure -state disabled
    }

    proc switch_to {chan} {
        variable cur
        variable joined
        variable oldest ""
        variable newest ""
        variable unread
        set cur $chan
        if {$chan ni $joined} { lappend joined $chan }
        ocaml::remember cur $cur
        ocaml::remember joined $joined
        set unread($chan) 0
        clear_log
        ocaml::messages $chan "" [list ui::show_history $chan]
        refresh_chans
        announce
        focus .main.pw.right.input
    }

    proc show_history {chan msgs} {
        variable cur
        variable oldest
        variable newest
        if {$chan ne $cur} return
        # replaces whatever on_event added in the meantime
        clear_log
        foreach m $msgs { render $m end }
        if {[llength $msgs]} {
            set oldest [dict get [lindex $msgs 0] id]
            set newest [dict get [lindex $msgs end] id]
        }
        [log] see end
    }

    proc load_older {} {
        variable cur
        variable oldest
        if {$oldest eq ""} return
        ocaml::messages $cur $oldest [list ui::show_older $cur]
    }

    proc show_older {chan msgs} {
        variable cur
        variable oldest
        if {$chan ne $cur || ![llength $msgs]} return
        foreach m [lreverse $msgs] { render $m 1.0 }
        set oldest [dict get [lindex $msgs 0] id]
        [log] see 1.0
    }

    proc announce {} {
        variable joined
        ocaml::presence {*}$joined {}
    }

    proc on_presence {p} {
        variable seen
        set seen([dict get $p author]) [list [clock seconds] [dict get $p chans]]
        show_here
    }

    # every 5s: announce ourselves, drop whoever went silent
    proc tick {} {
        variable tick [after 5000 ui::tick]
        announce
        show_here
    }

    # fill the "Here" list for the current channel
    proc show_here {} {
        variable cur
        variable seen
        set here {}
        foreach {a v} [array get seen] {
            lassign $v at chans
            if {[clock seconds] - $at > 40} {
                unset seen($a)
            } elseif {$cur in $chans} {
                lappend here $a
            }
        }
        set lb .main.pw.users.list
        $lb delete 0 end
        foreach a [lsort $here] {
            $lb insert end $a
            $lb itemconfigure end -foreground [color $a]
        }
    }

    # the daemon (re)started: its history may be gone, redraw from scratch
    proc on_connected {args} {
        variable cur
        update_status
        if {$::ocaml::state(connected)} {
            fetch_chans
            switch_to $cur
        }
    }

    proc on_select {} {
        variable shown
        variable cur
        set id [lindex [.main.pw.left.chans selection] 0]
        if {$id eq ""} return
        set chan [lindex $shown [string range $id 1 end]]
        # refresh_chans re-selects the current channel, which lands here again
        if {$chan ne $cur} { switch_to $chan }
    }

    # select the previous (-1) or next (+1) channel in the list
    proc move_chan {delta} {
        variable shown
        variable cur
        set i [expr {[lsearch -exact $shown $cur] + $delta}]
        if {$i >= 0 && $i < [llength $shown]} { switch_to [lindex $shown $i] }
    }

    proc join_new {} {
        set e .main.pw.left.new
        set name [string trim [$e get]]
        if {$name eq ""} return
        $e delete 0 end
        switch_to $name
    }

    proc send {} {
        variable cur
        set e .main.pw.right.input
        set text [string trim [$e get]]
        if {$text eq ""} return
        $e delete 0 end
        # no local echo: the daemon pushes it back to us via ui::on_event
        ocaml::post $cur $text {}
    }

    proc on_event {e} {
        set m [dict get $e msg]
        variable cur
        variable known
        variable oldest
        variable newest
        variable unread
        set chan [dict get $m chan]
        set id [dict get $m id]
        if {$chan ni $known} { lappend known $chan }
        if {$chan eq $cur} {
            # may already be part of the history we just loaded
            if {$newest ne "" && $id <= $newest} return
            set t [log]
            set at_bottom [expr {[lindex [$t yview] 1] >= 0.999}]
            render $m end
            set newest $id
            if {$oldest eq ""} { set oldest $id }
            if {$at_bottom} { $t see end }
        } else {
            incr unread($chan)
            refresh_chans
        }
    }
}

ocaml::on event ui::on_event
ocaml::on presence ui::on_presence
ocaml::watch connected ui::on_connected
ocaml::watch outbox ui::update_status

ui::update_status
ui::fetch_chans
ui::switch_to $ui::cur
ui::tick
