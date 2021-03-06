// Copyright 2014 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

// This compiler pass detects static items that refer to themselves
// recursively.

use ast_map;
use session::Session;
use middle::def::{DefStatic, DefConst, DefAssociatedConst, DefMap};

use syntax::{ast, ast_util};
use syntax::codemap::Span;
use syntax::visit::Visitor;
use syntax::visit;

struct CheckCrateVisitor<'a, 'ast: 'a> {
    sess: &'a Session,
    def_map: &'a DefMap,
    ast_map: &'a ast_map::Map<'ast>
}

impl<'v, 'a, 'ast> Visitor<'v> for CheckCrateVisitor<'a, 'ast> {
    fn visit_item(&mut self, it: &ast::Item) {
        match it.node {
            ast::ItemStatic(_, _, ref expr) |
            ast::ItemConst(_, ref expr) => {
                let mut recursion_visitor =
                    CheckItemRecursionVisitor::new(self, &it.span);
                recursion_visitor.visit_item(it);
                visit::walk_expr(self, &*expr)
            },
            _ => visit::walk_item(self, it)
        }
    }

    fn visit_trait_item(&mut self, ti: &ast::TraitItem) {
        match ti.node {
            ast::ConstTraitItem(_, ref default) => {
                if let Some(ref expr) = *default {
                    let mut recursion_visitor =
                        CheckItemRecursionVisitor::new(self, &ti.span);
                    recursion_visitor.visit_trait_item(ti);
                    visit::walk_expr(self, &*expr)
                }
            }
            _ => visit::walk_trait_item(self, ti)
        }
    }

    fn visit_impl_item(&mut self, ii: &ast::ImplItem) {
        match ii.node {
            ast::ConstImplItem(_, ref expr) => {
                let mut recursion_visitor =
                    CheckItemRecursionVisitor::new(self, &ii.span);
                recursion_visitor.visit_impl_item(ii);
                visit::walk_expr(self, &*expr)
            }
            _ => visit::walk_impl_item(self, ii)
        }
    }
}

pub fn check_crate<'ast>(sess: &Session,
                         krate: &ast::Crate,
                         def_map: &DefMap,
                         ast_map: &ast_map::Map<'ast>) {
    let mut visitor = CheckCrateVisitor {
        sess: sess,
        def_map: def_map,
        ast_map: ast_map
    };
    visit::walk_crate(&mut visitor, krate);
    sess.abort_if_errors();
}

struct CheckItemRecursionVisitor<'a, 'ast: 'a> {
    root_span: &'a Span,
    sess: &'a Session,
    ast_map: &'a ast_map::Map<'ast>,
    def_map: &'a DefMap,
    idstack: Vec<ast::NodeId>
}

impl<'a, 'ast: 'a> CheckItemRecursionVisitor<'a, 'ast> {
    fn new(v: &CheckCrateVisitor<'a, 'ast>, span: &'a Span)
           -> CheckItemRecursionVisitor<'a, 'ast> {
        CheckItemRecursionVisitor {
            root_span: span,
            sess: v.sess,
            ast_map: v.ast_map,
            def_map: v.def_map,
            idstack: Vec::new()
        }
    }
    fn with_item_id_pushed<F>(&mut self, id: ast::NodeId, f: F)
          where F: Fn(&mut Self) {
        if self.idstack.iter().any(|x| x == &(id)) {
            span_err!(self.sess, *self.root_span, E0265, "recursive constant");
            return;
        }
        self.idstack.push(id);
        f(self);
        self.idstack.pop();
    }
}

impl<'a, 'ast, 'v> Visitor<'v> for CheckItemRecursionVisitor<'a, 'ast> {
    fn visit_item(&mut self, it: &ast::Item) {
        self.with_item_id_pushed(it.id, |v| visit::walk_item(v, it));
    }

    fn visit_trait_item(&mut self, ti: &ast::TraitItem) {
        self.with_item_id_pushed(ti.id, |v| visit::walk_trait_item(v, ti));
    }

    fn visit_impl_item(&mut self, ii: &ast::ImplItem) {
        self.with_item_id_pushed(ii.id, |v| visit::walk_impl_item(v, ii));
    }

    fn visit_expr(&mut self, e: &ast::Expr) {
        match e.node {
            ast::ExprPath(..) => {
                match self.def_map.borrow().get(&e.id).map(|d| d.base_def) {
                    Some(DefStatic(def_id, _)) |
                    Some(DefAssociatedConst(def_id, _)) |
                    Some(DefConst(def_id)) if
                            ast_util::is_local(def_id) => {
                        match self.ast_map.get(def_id.node) {
                          ast_map::NodeItem(item) =>
                            self.visit_item(item),
                          ast_map::NodeTraitItem(item) =>
                            self.visit_trait_item(item),
                          ast_map::NodeImplItem(item) =>
                            self.visit_impl_item(item),
                          ast_map::NodeForeignItem(_) => {},
                          _ => {
                            span_err!(self.sess, e.span, E0266,
                              "expected item, found {}",
                                      self.ast_map.node_to_string(def_id.node));
                            return;
                          },
                        }
                    }
                    _ => ()
                }
            },
            _ => ()
        }
        visit::walk_expr(self, e);
    }
}
