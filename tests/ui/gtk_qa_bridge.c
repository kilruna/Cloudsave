/* Test-only LD_PRELOAD bridge. Executes widget actions on GTK's main thread.
 * No application functions, sync backends, config reads or results are mocked. */
#define _GNU_SOURCE
#include <dlfcn.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <unistd.h>
#include <stdint.h>
typedef void *P;
typedef unsigned long T;
static char dir[2048];
static P nodes[8192];
static int count;
#define FN(ret,name,args) static ret (*name) args
FN(unsigned,g_timeout_add,(unsigned,int(*)(P),P));
FN(P,gtk_window_get_toplevels,(void));
FN(unsigned,g_list_model_get_n_items,(P));
FN(P,g_list_model_get_item,(P,unsigned));
FN(void,g_object_unref,(P));
FN(const char*,g_type_name_from_instance,(P));
FN(T,g_type_from_name,(const char*));
FN(int,g_type_check_instance_is_a,(P,T));
FN(P,gtk_widget_get_first_child,(P));
FN(P,gtk_widget_get_next_sibling,(P));
FN(int,gtk_widget_get_visible,(P));
FN(int,gtk_widget_get_mapped,(P));
FN(int,gtk_widget_is_sensitive,(P));
FN(int,gtk_widget_get_width,(P));
FN(int,gtk_widget_get_height,(P));
FN(const char*,gtk_label_get_text,(P));
FN(const char*,gtk_button_get_label,(P));
FN(int,gtk_check_button_get_active,(P));
FN(void,gtk_check_button_set_active,(P,int));
FN(int,gtk_switch_get_active,(P));
FN(void,gtk_switch_set_active,(P,int));
FN(double,gtk_spin_button_get_value,(P));
FN(void,gtk_spin_button_set_value,(P,double));
FN(void,gtk_editable_set_text,(P,const char*));
FN(const char*,gtk_editable_get_text,(P));
FN(T,gtk_editable_get_type,(void));
FN(void,g_signal_emit_by_name,(P,const char*,...));
FN(int,gtk_widget_activate,(P));
FN(void,gtk_window_close,(P));
FN(void,gtk_drop_down_set_selected,(P,unsigned));
static int is(P w,const char*t){ T n=g_type_from_name(t);return n && g_type_check_instance_is_a(w,n); }
static void text(FILE*f,const char*s){if(s)for(;*s;s++)fputc((*s=='\t'||*s=='\n'||*s=='\r')?' ':*s,f);}
static void visit(P w,int parent,FILE*f){
 if(!w||count>=8192)return;
 int id=count;nodes[count++]=w;
 if(f){
  const char* label="";
  if(is(w,"GtkLabel"))label=gtk_label_get_text(w);
  else if(is(w,"GtkButton"))label=gtk_button_get_label(w);
  int active=-1;
  if(is(w,"GtkCheckButton"))active=gtk_check_button_get_active(w);
  if(is(w,"GtkSwitch"))active=gtk_switch_get_active(w);
  fprintf(f,"%d\t%d\t%s\t%d\t%d\t%d\t%d\t%d\t%d\t",id,parent,g_type_name_from_instance(w),gtk_widget_get_visible(w),gtk_widget_get_mapped(w),gtk_widget_is_sensitive(w),active,gtk_widget_get_width(w),gtk_widget_get_height(w));
  text(f,label);fputc('\t',f);
  if(g_type_check_instance_is_a(w,gtk_editable_get_type()))text(f,gtk_editable_get_text(w));
  fprintf(f,"\t%.0f\n",is(w,"GtkSpinButton")?gtk_spin_button_get_value(w):0.0);
 }
 for(P c=gtk_widget_get_first_child(w);c;c=gtk_widget_get_next_sibling(c))visit(c,id,f);
}
static void tree(FILE*f){
 count=0;P m=gtk_window_get_toplevels();
 for(unsigned i=0;i<g_list_model_get_n_items(m);i++){P w=g_list_model_get_item(m,i);visit(w,-1,f);g_object_unref(w);}
}
static int tick(P unused){
 (void)unused;char path[2300],out[2300];snprintf(path,sizeof(path),"%s/command",dir);
 FILE*f=fopen(path,"r");if(!f)return 1;
 char line[4096];if(!fgets(line,sizeof(line),f)){fclose(f);return 1;}fclose(f);unlink(path);
 char *save=NULL,*seq=strtok_r(line,"\t\n",&save),*verb=strtok_r(NULL,"\t\n",&save),*idx=strtok_r(NULL,"\t\n",&save),*val=strtok_r(NULL,"\n",&save);
 if(!seq||!verb)return 1;
 tree(NULL);int id=idx?atoi(idx):-1;P w=(id>=0&&id<count)?nodes[id]:NULL;int ok=1;
 if(!strcmp(verb,"dump")){}
 else if(!w)ok=0;
 else if(!strcmp(verb,"click")&&is(w,"GtkButton")&&gtk_widget_is_sensitive(w))g_signal_emit_by_name(w,"clicked");
 else if(!strcmp(verb,"active")&&is(w,"GtkCheckButton"))gtk_check_button_set_active(w,val?atoi(val):0);
 else if(!strcmp(verb,"active")&&is(w,"GtkSwitch"))gtk_switch_set_active(w,val?atoi(val):0);
 else if(!strcmp(verb,"selected")&&is(w,"GtkDropDown"))gtk_drop_down_set_selected(w,val?atoi(val):0);
 else if(!strcmp(verb,"value")&&is(w,"GtkSpinButton"))gtk_spin_button_set_value(w,val?atof(val):0);
 else if(!strcmp(verb,"text")&&g_type_check_instance_is_a(w,gtk_editable_get_type()))gtk_editable_set_text(w,val?val:"");
 else if(!strcmp(verb,"activate"))ok=gtk_widget_activate(w);
 else if(!strcmp(verb,"close")&&is(w,"GtkWindow"))gtk_window_close(w);
 else ok=0;
 snprintf(out,sizeof(out),"%s/tree.tsv.tmp",dir);f=fopen(out,"w");if(f){tree(f);fclose(f);snprintf(path,sizeof(path),"%s/tree.tsv",dir);rename(out,path);}
 snprintf(path,sizeof(path),"%s/response.tmp",dir);f=fopen(path,"w");if(f){fprintf(f,"%s\t%d\n",seq,ok);fclose(f);snprintf(out,sizeof(out),"%s/response",dir);rename(path,out);}
 return 1;
}
#define LOAD(n) do{*(void**)(&n)=dlsym(RTLD_DEFAULT,#n);if(!n)return;}while(0)
__attribute__((constructor))static void init(void){
 const char*d=getenv("CLOUDSAVE_QA_DIR");if(!d)return;snprintf(dir,sizeof(dir),"%s",d);
 LOAD(gtk_drop_down_set_selected);LOAD(g_timeout_add);LOAD(gtk_window_get_toplevels);LOAD(g_list_model_get_n_items);LOAD(g_list_model_get_item);LOAD(g_object_unref);LOAD(g_type_name_from_instance);LOAD(g_type_from_name);LOAD(g_type_check_instance_is_a);LOAD(gtk_widget_get_first_child);LOAD(gtk_widget_get_next_sibling);LOAD(gtk_widget_get_visible);LOAD(gtk_widget_get_mapped);LOAD(gtk_widget_is_sensitive);LOAD(gtk_widget_get_width);LOAD(gtk_widget_get_height);LOAD(gtk_label_get_text);LOAD(gtk_button_get_label);LOAD(gtk_check_button_get_active);LOAD(gtk_check_button_set_active);LOAD(gtk_switch_get_active);LOAD(gtk_switch_set_active);LOAD(gtk_spin_button_get_value);LOAD(gtk_spin_button_set_value);LOAD(gtk_editable_set_text);LOAD(gtk_editable_get_text);LOAD(gtk_editable_get_type);LOAD(g_signal_emit_by_name);LOAD(gtk_widget_activate);LOAD(gtk_window_close);
 g_timeout_add(50,tick,NULL);
}
