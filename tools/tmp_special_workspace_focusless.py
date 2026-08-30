from pathlib import Path

p = Path("src/desktop/service.rs")
s = p.read_text(encoding="utf-8")

old = '''            if Some(current) == self.special_workspace {
                let target =
                    choose_special_return_target(self.special_return, self.history.current(), &ids)
                        .ok_or_else(|| {
                            DesktopError::NavigationUnavailable(
                                "no normal desktop is available to leave the special workspace"
                                    .into(),
                            )
                        })?;
                self.native
                    .as_ref()
                    .expect("native backend was checked above")
                    .switch_to_id(target)?;
                self.special_return = None;
                self.history.observe_desktop(target);
                return Ok(target);
            }
'''
new = '''            if Some(current) == self.special_workspace {
                let target =
                    choose_special_return_target(self.special_return, self.history.current(), &ids)
                        .ok_or_else(|| {
                            DesktopError::NavigationUnavailable(
                                "no normal desktop is available to leave the special workspace"
                                    .into(),
                            )
                        })?;
                self.native
                    .as_ref()
                    .expect("native backend was checked above")
                    .switch_to_id(target)?;
                self.special_return = None;
                self.history.observe_desktop(target);
                return Ok((target, true));
            }
'''
if s.count(old) != 1:
    raise SystemExit(f"special Previous branch: expected 1 match, got {s.count(old)}")
s = s.replace(old, new, 1)

old = '''            self.history.note_previous_switch(current, target);
            Ok(target)
        })();
        match result {
            Ok(target) => {
                self.last_served = Some(BackendKind::NativeShell);
                if let Err(error) = self.restore_focus(target) {
                    self.publish_failure("restore previous desktop focus", error);
                }
                self.publish_status();
            }
'''
new = '''            self.history.note_previous_switch(current, target);
            Ok((target, false))
        })();
        match result {
            Ok((target, left_special_workspace)) => {
                self.last_served = Some(BackendKind::NativeShell);
                if !left_special_workspace {
                    if let Err(error) = self.restore_focus(target) {
                        self.publish_failure("restore previous desktop focus", error);
                    }
                }
                self.publish_status();
            }
'''
if s.count(old) != 1:
    raise SystemExit(f"Previous result branch: expected 1 match, got {s.count(old)}")
s = s.replace(old, new, 1)

old = '''                self.special_return = None;
                self.history.observe_desktop(target);
                Ok((target, false))
            } else {
                self.native
                    .as_ref()
                    .expect("native backend was checked above")
                    .switch_to_id(workspace)?;
                self.special_return = Some(current);
                Ok((workspace, true))
            }
        })();
        match result {
            Ok((target, entering)) => {
                self.last_served = Some(BackendKind::NativeShell);
                if let Err(error) = self.restore_focus(target) {
                    self.publish_failure(
                        if entering {
                            "restore special workspace focus"
                        } else {
                            "restore normal desktop focus"
                        },
                        error,
                    );
                }
                self.publish_status();
                crate::info!(
                    "{} special workspace",
                    if entering { "entered" } else { "left" }
                );
            }
'''
new = '''                self.special_return = None;
                self.history.observe_desktop(target);
                Ok(false)
            } else {
                self.native
                    .as_ref()
                    .expect("native backend was checked above")
                    .switch_to_id(workspace)?;
                self.special_return = Some(current);
                Ok(true)
            }
        })();
        match result {
            Ok(entering) => {
                self.last_served = Some(BackendKind::NativeShell);
                // This is a real Virtual Desktop transition. Deliberately let
                // Shell own foreground/focus selection instead of replaying
                // Phase-1 SetForegroundWindow restoration here.
                self.publish_status();
                crate::info!(
                    "{} special workspace",
                    if entering { "entered" } else { "left" }
                );
            }
'''
if s.count(old) != 1:
    raise SystemExit(f"Toggle result branch: expected 1 match, got {s.count(old)}")
s = s.replace(old, new, 1)

p.write_text(s, encoding="utf-8")
print("special workspace transitions now leave focus to Shell")
