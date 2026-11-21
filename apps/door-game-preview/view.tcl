# Scene presentation shared by the interactive canvas and offline snapshot.
namespace eval preview {
    variable width 960
    variable height 650
    variable boxRows {}
    variable frame {}
    variable stepMs 20
}

proc preview::receive {line} {
    variable boxRows
    variable frame
    variable stepMs
    set fields [split $line]
    switch -- [lindex $fields 0] {
        step-ms { set stepMs [lindex $fields 1] }
        box { lappend boxRows [lrange $fields 1 end] }
        frame { set frame [lrange $fields 1 end] }
        default { error "Unexpected response from Door Game: $line" }
    }
}

proc preview::project {x y z} {
    return [list [expr {330 + 45*$x - 20*$z}] [expr {357 + 11*$x + 30*$z - 37*$y}]]
}

proc preview::shade {colour factor} {
    scan $colour "#%2x%2x%2x" red green blue
    return [format "#%02x%02x%02x" \
        [expr {min(255, round($red*$factor))}] \
        [expr {min(255, round($green*$factor))}] \
        [expr {min(255, round($blue*$factor))}]]
}

proc preview::box {draw row} {
    lassign $row id x y z w h d colour
    set xa [expr {$x-$w/2.0}]
    set xb [expr {$x+$w/2.0}]
    set ya [expr {$y-$h/2.0}]
    set yb [expr {$y+$h/2.0}]
    set za [expr {$z-$d/2.0}]
    set zb [expr {$z+$d/2.0}]
    set a [project $xa $yb $za]
    set b [project $xb $yb $za]
    set c [project $xb $yb $zb]
    set d [project $xa $yb $zb]
    set e [project $xa $ya $zb]
    set f [project $xb $ya $zb]
    set g [project $xb $ya $za]
    set line [shade $colour 0.45]
    {*}$draw poly [concat $d $c $f $e] [shade $colour 0.62] $line
    {*}$draw poly [concat $b $c $f $g] [shade $colour 0.48] $line
    {*}$draw poly [concat $a $b $c $d] $colour $line
}

proc preview::sortDepth {first second} {
    set a [expr {[lindex $first 3] + 0.36*[lindex $first 1]}]
    set b [expr {[lindex $second 3] + 0.36*[lindex $second 1]}]
    return [expr {($a > $b) - ($a < $b)}]
}

proc preview::grid {draw row} {
    lassign $row id x y z w h d colour
    set top [expr {$y + $h/2.0}]
    set x1 [expr {$x - $w/2.0}]
    set x2 [expr {$x + $w/2.0}]
    set z1 [expr {$z - $d/2.0}]
    set z2 [expr {$z + $d/2.0}]
    set gridColour [shade $colour 0.82]
    # Limit strokes for very large authored floors.
    for {set tick 1} {$tick < 20} {incr tick} {
        set px [expr {$x1 + $tick*$w/20.0}]
        set pz [expr {$z1 + $tick*$d/20.0}]
        {*}$draw line {*}[project $px $top $z1] {*}[project $px $top $z2] $gridColour
        {*}$draw line {*}[project $x1 $top $pz] {*}[project $x2 $top $pz] $gridColour
    }
}

proc preview::diagram {draw} {
    variable boxRows
    variable frame
    lassign $frame x y z open plate timer won
    set geometry {}
    foreach row $boxRows {
        set id [lindex $row 0]
        switch -- $id {
            floor {
                box $draw $row
                grid $draw $row
            }
            player {
                lset row 1 $x
                lset row 2 $y
                lset row 3 $z
                lappend geometry $row
            }
            door {
                if {$open} {
                    set doorHeight [lindex $row 5]
                    lset row 2 [expr {[lindex $row 2] + $doorHeight}]
                }
                lappend geometry $row
            }
            default { lappend geometry $row }
        }
    }
    foreach row [lsort -command preview::sortDepth $geometry] {
        box $draw $row
    }
}

proc preview::draw {draw} {
    variable frame
    variable width
    variable height
    lassign $frame x y z open plate timer won

    {*}$draw rect 0 0 $width $height #101720 #101720
    {*}$draw text 33 28 "STEELGUM" #90afd0 11 bold
    {*}$draw text 31 51 "Door Game" #f0f4f8 31 bold
    {*}$draw text 33 98 "Step on the blue plate, then reach the green goal." #a3b4c7 14 normal

    {*}$draw rect 29 140 631 582 #141f2b #33465a
    {*}$draw text 49 158 "ROOM PREVIEW" #9bb9d7 12 bold
    diagram $draw

    {*}$draw rect 650 140 932 582 #182433 #33465a
    {*}$draw text 671 160 "Run status" #eff5fb 18 bold
    if {$won} {
        set message "Goal reached"
        set accent #70dfae
    } elseif {$plate} {
        set message "Pressure plate held"
        set accent #81c6ff
    } elseif {$open} {
        set message "Door open"
        set accent #ffd193
    } else {
        set message "Door closed"
        set accent #ffd193
    }
    {*}$draw rect 671 195 910 230 #23374a #23374a
    {*}$draw text 682 206 $message $accent 15 bold
    set displayTimer [format "%.1f s" [expr {$timer/1000.0}]]
    {*}$draw text 671 253 "Door timer" #a3b4c7 13 normal
    {*}$draw text 832 253 $displayTimer #eff5fb 13 bold
    {*}$draw rect 671 278 909 284 #314459 #314459
    set end [expr {671 + min(238, $timer*238/3000.0)}]
    if {$end > 671} {
        {*}$draw rect 671 278 $end 284 $accent $accent
    }
    {*}$draw line 671 311 910 311 #34475a
    foreach {top colour label} {
        339 #f3e7b3 Player
        374 #2d95ec {Pressure plate}
        409 #ef812f Door
        444 #42c884 Goal
    } {
        {*}$draw rect 671 $top 683 [expr {$top+12}] $colour $colour
        {*}$draw text 698 [expr {$top-2}] $label #c2d0df 13 normal
    }
    {*}$draw rect 671 494 910 527 #193e60 #4e83af
    {*}$draw text 689 503 "Watch demo" #e9f5ff 14 bold
    {*}$draw rect 671 537 910 567 #26384c #435971
    {*}$draw text 689 544 "Restart" #dce9f4 14 bold

    {*}$draw text 33 612 "Move   WASD / arrows" #bacada 13 normal
    {*}$draw text 286 612 "Watch   Space" #bacada 13 normal
    {*}$draw text 481 612 "Restart   R" #bacada 13 normal
    {*}$draw text 669 612 "Quit   Esc" #bacada 13 normal
}
